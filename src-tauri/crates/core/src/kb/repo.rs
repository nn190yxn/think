//! 知识库仓储：来源、文档、主题、全文索引与统计查询。

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, Transaction};

use crate::error::CoreResult;
use crate::util::unique_id;

use super::{
    KbDocumentView, KbDomainStat, KbFilter, KbRingStat, KbSearchHit, KbSourceView, KbTopicView,
    DEFAULT_DOC_LIMIT, MAX_DOC_LIMIT, RING_MONTHS,
};

const MIN_FTS_LEN: usize = 3;

pub fn now(conn: &Connection) -> CoreResult<String> {
    let value: String = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [],
        |row| row.get(0),
    )?;
    Ok(value)
}

pub fn path_available(path: &str) -> bool {
    Path::new(path).is_dir()
}

// ---------- 来源 ----------

fn read_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<KbSourceView> {
    let path: String = row.get(1)?;
    Ok(KbSourceView {
        id: row.get(0)?,
        available: row.get::<_, i64>(2)? != 0 && path_available(&path),
        path,
        paused: row.get::<_, i64>(3)? != 0,
        last_scan_at: row.get(4)?,
        last_success_at: row.get(5)?,
        doc_count: row.get(6)?,
    })
}

const SOURCE_COLUMNS: &str =
    "s.id, s.path, s.available, s.paused, s.last_scan_at, s.last_success_at,
     (SELECT COUNT(*) FROM kb_documents d WHERE d.source_id = s.id)";

pub fn list_sources(conn: &Connection) -> CoreResult<Vec<KbSourceView>> {
    let sql = format!("SELECT {SOURCE_COLUMNS} FROM kb_sources s ORDER BY s.created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], read_source)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

pub fn get_source(conn: &Connection, id: &str) -> CoreResult<KbSourceView> {
    let sql = format!("SELECT {SOURCE_COLUMNS} FROM kb_sources s WHERE s.id = ?1");
    conn.query_row(&sql, [id], read_source)
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                crate::error::CoreError::NotFound(format!("知识库来源不存在：{id}"))
            }
            other => other.into(),
        })
}

/// 登记来源。路径重复时直接返回既有记录。
pub fn add_source(conn: &Connection, path: &str) -> CoreResult<KbSourceView> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(crate::error::CoreError::InvalidInput(
            "知识库来源路径不能为空".to_string(),
        ));
    }
    let mut stmt = conn.prepare("SELECT id FROM kb_sources WHERE path = ?1")?;
    let existing: Option<String> = stmt
        .query_row([trimmed], |row| row.get(0))
        .ok();
    if let Some(id) = existing {
        return get_source(conn, &id);
    }
    let id = unique_id("kb-src", trimmed);
    conn.execute(
        "INSERT INTO kb_sources (id, path, available, paused, created_at)
         VALUES (?1, ?2, ?3, 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        rusqlite::params![id, trimmed, i64::from(path_available(trimmed))],
    )?;
    get_source(conn, &id)
}

pub fn remove_source(conn: &Connection, id: &str) -> CoreResult<bool> {
    let affected = conn.execute("DELETE FROM kb_sources WHERE id = ?1", [id])?;
    conn.execute(
        "DELETE FROM kb_search WHERE doc_id IN (SELECT id FROM kb_documents WHERE source_id = ?1)",
        [id],
    )?;
    conn.execute("DELETE FROM kb_documents WHERE source_id = ?1", [id])?;
    recompute_topic_counts(conn)?;
    Ok(affected > 0)
}

pub fn set_source_availability(
    conn: &Connection,
    id: &str,
    available: bool,
    success: bool,
) -> CoreResult<()> {
    if success {
        conn.execute(
            "UPDATE kb_sources
             SET available = ?2,
                 last_scan_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                 last_success_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?1",
            rusqlite::params![id, i64::from(available)],
        )?;
    } else {
        conn.execute(
            "UPDATE kb_sources
             SET available = ?2, last_scan_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?1",
            rusqlite::params![id, i64::from(available)],
        )?;
    }
    Ok(())
}

/// 来源离线时把它的文档标为不可用，但保留记录可读。
pub fn mark_documents_availability(
    conn: &Connection,
    source_id: &str,
    available: bool,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE kb_documents SET available = ?2 WHERE source_id = ?1",
        rusqlite::params![source_id, i64::from(available)],
    )?;
    Ok(())
}

// ---------- 主题 ----------

pub fn upsert_topic(tx: &Transaction<'_>, display_name: &str) -> CoreResult<String> {
    let mut stmt = tx.prepare("SELECT id FROM kb_topics WHERE display_name = ?1")?;
    let existing: Option<String> = stmt.query_row([display_name], |row| row.get(0)).ok();
    drop(stmt);
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = unique_id("kb-topic", display_name);
    tx.execute(
        "INSERT INTO kb_topics (id, display_name, doc_count) VALUES (?1, ?2, 0)",
        rusqlite::params![id, display_name],
    )?;
    Ok(id)
}

/// 重新汇总主题文档数与最近修改时间。
pub fn recompute_topic_counts(conn: &Connection) -> CoreResult<()> {
    conn.execute(
        "UPDATE kb_topics SET doc_count = 0, latest_created_at = NULL, latest_modified_at = NULL",
        [],
    )?;
    conn.execute(
        "UPDATE kb_topics SET
             doc_count = (SELECT COUNT(*) FROM kb_documents d WHERE d.topic_id = kb_topics.id),
             latest_created_at = (SELECT MAX(created_at) FROM kb_documents d WHERE d.topic_id = kb_topics.id),
             latest_modified_at = (SELECT MAX(modified_at) FROM kb_documents d WHERE d.topic_id = kb_topics.id)",
        [],
    )?;
    conn.execute("DELETE FROM kb_topics WHERE doc_count = 0", [])?;
    Ok(())
}

pub fn list_topics(conn: &Connection, limit: i64) -> CoreResult<Vec<KbTopicView>> {
    let mut stmt = conn.prepare(
        "SELECT id, display_name, doc_count, latest_modified_at FROM kb_topics
         ORDER BY doc_count DESC, display_name ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit.clamp(1, MAX_DOC_LIMIT)], |row| {
        Ok(KbTopicView {
            id: row.get(0)?,
            display_name: row.get(1)?,
            doc_count: row.get(2)?,
            latest_modified_at: row.get(3)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

// ---------- 文档 ----------

pub struct DocumentUpsert<'a> {
    pub source_id: &'a str,
    pub path: &'a str,
    pub normalized_name: &'a str,
    pub version_label: &'a str,
    pub domain: &'a str,
    pub topic_id: &'a str,
    pub topic_display: &'a str,
    pub file_size: i64,
    pub metadata_hash: &'a str,
    pub created_at: Option<&'a str>,
    pub modified_at: Option<&'a str>,
}

/// 写入或更新一篇文档，维护全文索引，返回是否为新增。
pub fn upsert_document(tx: &Transaction<'_>, input: &DocumentUpsert<'_>) -> CoreResult<bool> {
    let mut stmt = tx.prepare(
        "SELECT id, metadata_hash FROM kb_documents WHERE source_id = ?1 AND path = ?2",
    )?;
    let existing: Option<(String, String)> = stmt
        .query_row(rusqlite::params![input.source_id, input.path], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .ok();
    drop(stmt);

    let now = now(tx)?;
    let id = match existing {
        Some((id, _)) => {
            tx.execute(
                "UPDATE kb_documents SET
                     normalized_name = ?2, version_label = ?3, domain = ?4, topic_id = ?5,
                     file_size = ?6, metadata_hash = ?7, created_at = ?8, modified_at = ?9,
                     available = 1, indexed_at = ?10
                 WHERE id = ?1",
                rusqlite::params![
                    id,
                    input.normalized_name,
                    input.version_label,
                    input.domain,
                    input.topic_id,
                    input.file_size,
                    input.metadata_hash,
                    input.created_at,
                    input.modified_at,
                    now
                ],
            )?;
            sync_search(
                tx,
                &id,
                input.normalized_name,
                input.normalized_name,
                input.path,
                input.topic_display,
            )?;
            return Ok(false);
        }
        None => unique_id("kb-doc", input.path),
    };

    tx.execute(
        "INSERT INTO kb_documents
             (id, source_id, path, normalized_name, version_label, domain, topic_id,
              file_size, metadata_hash, created_at, modified_at, available, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12)",
        rusqlite::params![
            id,
            input.source_id,
            input.path,
            input.normalized_name,
            input.version_label,
            input.domain,
            input.topic_id,
            input.file_size,
            input.metadata_hash,
            input.created_at,
            input.modified_at,
            now
        ],
    )?;
    sync_search(
        tx,
        &id,
        input.normalized_name,
        input.normalized_name,
        input.path,
        input.topic_display,
    )?;
    Ok(true)
}

pub fn sync_search(
    tx: &Transaction<'_>,
    doc_id: &str,
    title: &str,
    normalized_name: &str,
    path: &str,
    topic: &str,
) -> CoreResult<()> {
    tx.execute("DELETE FROM kb_search WHERE doc_id = ?1", [doc_id])?;
    tx.execute(
        "INSERT INTO kb_search (doc_id, title, normalized_name, path, topic)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![doc_id, title, normalized_name, path, topic],
    )?;
    Ok(())
}

/// 删除本次扫描未出现的文档，返回删除数量。
pub fn remove_missing_documents(
    tx: &Transaction<'_>,
    source_id: &str,
    keep: &HashSet<String>,
) -> CoreResult<i64> {
    let mut stmt = tx.prepare("SELECT id, path FROM kb_documents WHERE source_id = ?1")?;
    let rows = stmt.query_map([source_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut stale: Vec<(String, String)> = Vec::new();
    for row in rows {
        let (id, path) = row?;
        if !keep.contains(&path) {
            stale.push((id, path));
        }
    }
    drop(stmt);
    for (id, _) in &stale {
        tx.execute("DELETE FROM kb_search WHERE doc_id = ?1", [id])?;
        tx.execute("DELETE FROM kb_documents WHERE id = ?1", [id])?;
    }
    Ok(stale.len() as i64)
}

const DOC_COLUMNS: &str = "d.id, d.source_id, d.path, d.normalized_name, d.version_label,
 d.domain, d.topic_id, COALESCE(t.display_name, ''), d.file_size, d.created_at,
 d.modified_at, d.available";

fn read_document(row: &rusqlite::Row<'_>) -> rusqlite::Result<KbDocumentView> {
    Ok(KbDocumentView {
        id: row.get(0)?,
        source_id: row.get(1)?,
        path: row.get(2)?,
        normalized_name: row.get(3)?,
        version_label: row.get(4)?,
        domain: row.get(5)?,
        topic_id: row.get(6)?,
        topic_name: row.get(7)?,
        file_size: row.get(8)?,
        created_at: row.get(9)?,
        modified_at: row.get(10)?,
        available: row.get::<_, i64>(11)? != 0,
    })
}

pub fn list_documents(conn: &Connection, filter: &KbFilter) -> CoreResult<Vec<KbDocumentView>> {
    let limit = filter.limit.unwrap_or(DEFAULT_DOC_LIMIT).clamp(1, MAX_DOC_LIMIT);
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(source_id) = filter.source_id.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("d.source_id = ?{}", params.len() + 1));
        params.push(Box::new(source_id.to_string()));
    }
    if let Some(domain) = filter.domain.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("d.domain = ?{}", params.len() + 1));
        params.push(Box::new(domain.to_string()));
    }
    if let Some(topic_id) = filter.topic_id.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("d.topic_id = ?{}", params.len() + 1));
        params.push(Box::new(topic_id.to_string()));
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT {DOC_COLUMNS} FROM kb_documents d
         LEFT JOIN kb_topics t ON t.id = d.topic_id
         {where_clause} ORDER BY d.normalized_name ASC, d.path ASC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(
        params.iter().map(|param| param.as_ref()),
    ))?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(read_document(row)?);
    }
    Ok(items)
}

/// 主题检索：FTS5 优先，零命中或短词退回 LIKE。
pub fn search(conn: &Connection, query: &str, limit: Option<i64>) -> CoreResult<Vec<KbSearchHit>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.unwrap_or(DEFAULT_DOC_LIMIT).clamp(1, MAX_DOC_LIMIT);
    let mut hits = Vec::new();

    if trimmed.chars().count() >= MIN_FTS_LEN {
        let sql = format!(
            "SELECT {DOC_COLUMNS} FROM kb_documents d
             LEFT JOIN kb_topics t ON t.id = d.topic_id
             WHERE d.id IN (SELECT doc_id FROM kb_search WHERE kb_search MATCH ?1)
             ORDER BY d.normalized_name ASC LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![trimmed, limit], read_document)?;
        for row in rows {
            hits.push(KbSearchHit {
                document: row?,
                matched_by: "fts".to_string(),
            });
        }
    }

    if hits.is_empty() {
        let pattern = format!("%{trimmed}%");
        let sql = format!(
            "SELECT {DOC_COLUMNS} FROM kb_documents d
             LEFT JOIN kb_topics t ON t.id = d.topic_id
             WHERE d.normalized_name LIKE ?1 OR d.path LIKE ?1 OR t.display_name LIKE ?1
             ORDER BY d.normalized_name ASC LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![pattern, limit], read_document)?;
        for row in rows {
            hits.push(KbSearchHit {
                document: row?,
                matched_by: "like".to_string(),
            });
        }
    }
    Ok(hits)
}

pub fn domain_distribution(conn: &Connection) -> CoreResult<Vec<KbDomainStat>> {
    let mut stmt = conn.prepare(
        "SELECT CASE WHEN domain = '' THEN '未归类' ELSE domain END AS name, COUNT(*)
         FROM kb_documents GROUP BY name ORDER BY COUNT(*) DESC, name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(KbDomainStat {
            domain: row.get(0)?,
            doc_count: row.get(1)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

/// 近若干个月的年轮：每月新增量与累计量，按时间升序。
pub fn ring_trend(conn: &Connection, months: usize) -> CoreResult<Vec<KbRingStat>> {
    let months = months.clamp(1, 36) as i64;
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m', COALESCE(modified_at, created_at, indexed_at)) AS period,
                COUNT(*) AS added
         FROM kb_documents
         WHERE COALESCE(modified_at, created_at, indexed_at) IS NOT NULL
         GROUP BY period ORDER BY period ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut all: Vec<(String, i64)> = Vec::new();
    for row in rows {
        all.push(row?);
    }
    let start = all.len().saturating_sub(months as usize);
    let mut total: i64 = all[..start].iter().map(|(_, added)| *added).sum();
    let mut rings = Vec::new();
    for (period, added) in all[start..].iter() {
        total += added;
        rings.push(KbRingStat {
            period: period.clone(),
            added: *added,
            total,
        });
    }
    Ok(rings)
}

pub fn doc_count(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM kb_documents", [], |row| row.get(0))?;
    Ok(count)
}

pub fn topic_count(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM kb_topics", [], |row| row.get(0))?;
    Ok(count)
}

pub fn ring_months_default() -> usize {
    RING_MONTHS
}
