//! 知识库服务：目录扫描、名称规范化、主题归组与地形统计。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::corpus::repo::normalize_name;
use crate::error::CoreResult;
use crate::util::sha256_hex;

use super::repo::{self, DocumentUpsert};
use super::{
    display_stem, topic_key, version_label, KbFilter, KbScanOutcome, KbSearchHit, KbSourceView,
    KnowledgeOverview, RING_MONTHS, SUPPORTED_EXTENSIONS,
};

/// 递归深度上限，避免异常目录结构导致扫描失控。
const MAX_DEPTH: usize = 8;

struct FileMeta {
    path: String,
    name: String,
    domain: String,
    file_size: i64,
    created_at: Option<String>,
    modified_at: Option<String>,
}

pub fn add_source(conn: &Connection, path: &str) -> CoreResult<KbSourceView> {
    repo::add_source(conn, path)
}

pub fn list_sources(conn: &Connection) -> CoreResult<Vec<KbSourceView>> {
    repo::list_sources(conn)
}

pub fn remove_source(conn: &Connection, id: &str) -> CoreResult<bool> {
    repo::remove_source(conn, id)
}

/// 扫描一个来源。来源离线时保留既有索引并标记不可用，不删除任何文档。
pub fn scan_source(conn: &mut Connection, source_id: &str) -> CoreResult<KbScanOutcome> {
    let source = repo::get_source(conn, source_id)?;
    if !repo::path_available(&source.path) {
        repo::set_source_availability(conn, source_id, false, false)?;
        repo::mark_documents_availability(conn, source_id, false)?;
        return Ok(KbScanOutcome {
            source_id: source_id.to_string(),
            available: false,
            reason: Some("source_unavailable".to_string()),
            ..KbScanOutcome::default()
        });
    }

    let files = walk(Path::new(&source.path))?;
    let scanned = files.len() as i64;
    let mut added = 0i64;
    let mut updated = 0i64;
    let mut keep: HashSet<String> = HashSet::new();

    let tx = conn.transaction()?;
    for file in &files {
        let stem = display_stem(&file.name);
        let label = version_label(&stem);
        let key = topic_key(&stem, label.as_deref());
        let topic_display = if key.is_empty() { stem.clone() } else { key };
        let topic_id = repo::upsert_topic(&tx, &topic_display)?;
        let normalized = normalize_name(&file.name);
        let hash = sha256_hex(&format!(
            "{}|{}|{}",
            file.path,
            file.file_size,
            file.modified_at.clone().unwrap_or_default()
        ));
        let is_new = repo::upsert_document(
            &tx,
            &DocumentUpsert {
                source_id,
                path: &file.path,
                normalized_name: &normalized,
                version_label: label.as_deref().unwrap_or(""),
                domain: &file.domain,
                topic_id: &topic_id,
                topic_display: &topic_display,
                file_size: file.file_size,
                metadata_hash: &hash,
                created_at: file.created_at.as_deref(),
                modified_at: file.modified_at.as_deref(),
            },
        )?;
        if is_new {
            added += 1;
        } else {
            updated += 1;
        }
        keep.insert(file.path.clone());
    }
    let removed = repo::remove_missing_documents(&tx, source_id, &keep)?;
    repo::recompute_topic_counts(&tx)?;
    tx.commit()?;

    repo::set_source_availability(conn, source_id, true, true)?;
    Ok(KbScanOutcome {
        source_id: source_id.to_string(),
        available: true,
        scanned,
        added,
        updated,
        removed,
        skipped: 0,
        reason: None,
    })
}

/// 扫描全部来源，逐条返回结果。
pub fn scan_all(conn: &mut Connection) -> CoreResult<Vec<KbScanOutcome>> {
    let sources = repo::list_sources(conn)?;
    let mut outcomes = Vec::with_capacity(sources.len());
    for source in sources {
        outcomes.push(scan_source(conn, &source.id)?);
    }
    Ok(outcomes)
}

pub fn list_documents(
    conn: &Connection,
    filter: &KbFilter,
) -> CoreResult<Vec<super::KbDocumentView>> {
    repo::list_documents(conn, filter)
}

pub fn search(conn: &Connection, query: &str, limit: Option<i64>) -> CoreResult<Vec<KbSearchHit>> {
    repo::search(conn, query, limit)
}

/// 知识地形总览。
pub fn overview(conn: &Connection, topic_limit: i64) -> CoreResult<KnowledgeOverview> {
    let sources = repo::list_sources(conn)?;
    Ok(KnowledgeOverview {
        source_count: sources.len() as i64,
        available_sources: sources.iter().filter(|source| source.available).count() as i64,
        doc_count: repo::doc_count(conn)?,
        topic_count: repo::topic_count(conn)?,
        domains: repo::domain_distribution(conn)?,
        topics: repo::list_topics(conn, topic_limit)?,
        rings: repo::ring_trend(conn, RING_MONTHS)?,
    })
}

/// 递归收集受支持的文件元数据。隐藏目录与符号链接目录跳过。
fn walk(root: &Path) -> CoreResult<Vec<FileMeta>> {
    let mut files = Vec::new();
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];
    while let Some((dir, domain, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                let next_domain = if domain.is_empty() {
                    name
                } else {
                    domain.clone()
                };
                stack.push((path, next_domain, depth + 1));
                continue;
            }
            if !file_type.is_file() || !is_supported(&name) {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            files.push(FileMeta {
                path: path.to_string_lossy().to_string(),
                name,
                domain: domain.clone(),
                file_size: metadata.len() as i64,
                created_at: metadata.created().ok().and_then(system_time_to_rfc3339),
                modified_at: metadata.modified().ok().and_then(system_time_to_rfc3339),
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn is_supported(name: &str) -> bool {
    match name.rfind('.') {
        Some(index) if index + 1 < name.len() => SUPPORTED_EXTENSIONS
            .iter()
            .any(|ext| name[index + 1..].eq_ignore_ascii_case(ext)),
        _ => false,
    }
}

/// 系统时间转 UTC RFC3339（秒精度）。
fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    let seconds = time.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(epoch_to_rfc3339(seconds))
}

fn epoch_to_rfc3339(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// 天数转公历年月日（Howard Hinnant 的 civil_from_days）。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// 供界面默认使用的年轮月份数。
pub fn ring_months() -> usize {
    RING_MONTHS
}
