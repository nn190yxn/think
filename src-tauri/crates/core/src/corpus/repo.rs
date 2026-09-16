//! 原始语料仓储：登记、检索与引用溯源。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, Transaction};
use sha2::{Digest, Sha256};

use crate::corpus::{CorpusItemView, CorpusSearchHit};
use crate::error::CoreResult;
use crate::master::CitationView;

/// 元数据检索的最小全文长度。更短的查询退回路径匹配。
const MIN_FTS_LEN: usize = 3;
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub fn deterministic_id(kind: &str, source_ref: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0u8]);
    hasher.update(source_ref.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("corpus:{}", digest.chars().take(16).collect::<String>())
}

/// 名称规范化：去掉扩展名，统一小写并把分隔符折叠成空格。
pub fn normalize_name(name: &str) -> String {
    let stem = match name.rfind('.') {
        Some(index) if index > 0 => &name[..index],
        _ => name,
    };
    let mut out = String::with_capacity(stem.len());
    let mut last_space = true;
    for ch in stem.chars() {
        let normalized = if ch.is_alphanumeric() { ch } else { ' ' };
        if normalized == ' ' {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.extend(normalized.to_lowercase());
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn path_available(source_ref: &str) -> bool {
    Path::new(source_ref).is_file()
}

/// 语料文件当前是否可访问。来源缺失不影响大师包可用。
pub fn availability_map(conn: &Connection) -> CoreResult<BTreeMap<String, bool>> {
    let mut stmt = conn.prepare("SELECT id, source_ref FROM corpus_items")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (id, source_ref) = row?;
        map.insert(id, path_available(&source_ref));
    }
    Ok(map)
}

/// 把大师 id 追加到语料的反向索引上，便于按大师检索语料。
pub fn attach_master(tx: &Transaction<'_>, corpus_id: &str, master_id: &str) -> CoreResult<()> {
    let raw: String = tx.query_row(
        "SELECT master_ids_json FROM corpus_items WHERE id = ?1",
        [corpus_id],
        |row| row.get(0),
    )?;
    let mut ids: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
    if ids.iter().any(|id| id == master_id) {
        return Ok(());
    }
    ids.push(master_id.to_string());
    tx.execute(
        "UPDATE corpus_items SET master_ids_json = ?2 WHERE id = ?1",
        rusqlite::params![corpus_id, serde_json::to_string(&ids).unwrap_or_default()],
    )?;
    Ok(())
}

/// 同步元数据全文索引。FTS5 不感知主表变化，需要在写入时显式维护。
pub fn sync_search_index(
    tx: &Transaction<'_>,
    corpus_id: &str,
    title: &str,
    normalized_name: &str,
    source_ref: &str,
) -> CoreResult<()> {
    tx.execute(
        "DELETE FROM corpus_search WHERE corpus_id = ?1",
        [corpus_id],
    )?;
    tx.execute(
        "INSERT INTO corpus_search (corpus_id, title, normalized_name, source_ref)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![corpus_id, title, normalized_name, source_ref],
    )?;
    Ok(())
}

pub fn citations_of_unit(
    conn: &Connection,
    unit_id: &str,
    availability: &BTreeMap<String, bool>,
) -> CoreResult<Vec<CitationView>> {
    let mut stmt = conn.prepare(
        "SELECT corpus_item_id, excerpt, location FROM corpus_citations
         WHERE master_unit_id = ?1
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([unit_id], |row| {
        let corpus_item_id: Option<String> = row.get(0)?;
        let excerpt: String = row.get(1)?;
        let location: String = row.get(2)?;
        Ok((corpus_item_id, excerpt, location))
    })?;

    let mut citations = Vec::new();
    for row in rows {
        let (corpus_item_id, excerpt, location) = row?;
        let available = corpus_item_id
            .as_ref()
            .and_then(|id| availability.get(id).copied())
            .unwrap_or(false);
        citations.push(CitationView {
            corpus_item_id,
            excerpt,
            location,
            available,
        });
    }
    Ok(citations)
}

const ITEM_COLUMNS: &str = "id, source_kind, source_ref, title, normalized_name, location_hint,\
 content_hash, byte_size, registered_at, master_ids_json";

fn read_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<CorpusItemView> {
    let master_ids_json: String = row.get(9)?;
    let source_ref: String = row.get(2)?;
    Ok(CorpusItemView {
        id: row.get(0)?,
        source_kind: row.get(1)?,
        source_ref: source_ref.clone(),
        title: row.get(3)?,
        normalized_name: row.get(4)?,
        location_hint: row.get(5)?,
        content_hash: row.get(6)?,
        byte_size: row.get(7)?,
        available: path_available(&source_ref),
        master_ids: serde_json::from_str(&master_ids_json).unwrap_or_default(),
        registered_at: row.get(8)?,
    })
}

/// 按大师筛选原始语料。
pub fn list(conn: &Connection, master_id: Option<&str>) -> CoreResult<Vec<CorpusItemView>> {
    let sql =
        format!("SELECT {ITEM_COLUMNS} FROM corpus_items ORDER BY registered_at DESC, title ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], read_item)?;

    let mut items = Vec::new();
    for row in rows {
        let item = row?;
        if let Some(master_id) = master_id {
            if !item.master_ids.iter().any(|id| id == master_id) {
                continue;
            }
        }
        items.push(item);
    }
    Ok(items)
}

/// 语料检索。元数据走 FTS5，短查询或零命中时退回路径匹配。
pub fn search(
    conn: &Connection,
    query: &str,
    limit: Option<i64>,
) -> CoreResult<Vec<CorpusSearchHit>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let mut hits = Vec::new();
    if trimmed.chars().count() >= MIN_FTS_LEN {
        let sql = format!(
            "SELECT {ITEM_COLUMNS} FROM corpus_items
             WHERE id IN (SELECT corpus_id FROM corpus_search WHERE corpus_search MATCH ?1)
             ORDER BY title ASC LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![trimmed, limit], read_item)?;
        for row in rows {
            hits.push(CorpusSearchHit {
                item: row?,
                matched_by: "fts".to_string(),
            });
        }
    }

    if hits.is_empty() {
        let pattern = format!("%{trimmed}%");
        let sql = format!(
            "SELECT {ITEM_COLUMNS} FROM corpus_items
             WHERE title LIKE ?1 OR normalized_name LIKE ?1 OR source_ref LIKE ?1
             ORDER BY title ASC LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![pattern, limit], read_item)?;
        for row in rows {
            hits.push(CorpusSearchHit {
                item: row?,
                matched_by: "like".to_string(),
            });
        }
    }

    Ok(hits)
}

/// 来源缺失的语料条数，供界面提示。
pub fn unavailable_count(conn: &Connection) -> CoreResult<i64> {
    let items = list(conn, None)?;
    Ok(items.iter().filter(|item| !item.available).count() as i64)
}
