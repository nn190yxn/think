//! 采集仓储：事件、派生摘要、能力开关、审计与运行参数。

use rusqlite::{Connection, Transaction};

use crate::db::settings;
use crate::error::CoreResult;

use super::redact::RedactionRules;
use super::{
    CaptureAuditView, CaptureEventView, CaptureFilter, CaptureSummaryView, DEFAULT_DEDUP_SECONDS,
    MAX_EXCERPT_CHARS, PAUSE_SETTING_KEY, REDACTION_SETTING_KEY,
};

const DEDUP_SETTING_KEY: &str = "capture.dedup_seconds";
const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;

pub fn now(conn: &Connection) -> CoreResult<String> {
    let value: String = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [],
        |row| row.get(0),
    )?;
    Ok(value)
}

// ---------- 能力开关 ----------

/// 读取某类能力的开关与确认时间。缺失时补一行默认关闭的设置。
pub fn capability(conn: &Connection, kind: &str) -> CoreResult<(bool, Option<String>)> {
    let row = conn.query_row(
        "SELECT enabled, consented_at FROM capture_settings WHERE kind = ?1",
        [kind],
        |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, Option<String>>(1)?)),
    );
    match row {
        Ok(value) => Ok(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            conn.execute(
                "INSERT OR IGNORE INTO capture_settings (kind, enabled, updated_at)
                 VALUES (?1, 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
                [kind],
            )?;
            Ok((false, None))
        }
        Err(error) => Err(error.into()),
    }
}

/// 切换某项能力。开启时记录显式确认时间。
pub fn set_capability(conn: &Connection, kind: &str, enabled: bool) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO capture_settings (kind, enabled, updated_at, consented_at)
         VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                 CASE WHEN ?2 = 1 THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ELSE NULL END)
         ON CONFLICT(kind) DO UPDATE SET
             enabled = excluded.enabled,
             updated_at = excluded.updated_at,
             consented_at = CASE
                 WHEN excluded.enabled = 1 THEN excluded.updated_at
                 ELSE capture_settings.consented_at
             END",
        rusqlite::params![kind, i64::from(enabled)],
    )?;
    Ok(())
}

/// 全局暂停状态。
pub fn paused(conn: &Connection) -> CoreResult<bool> {
    Ok(settings::get(conn, PAUSE_SETTING_KEY)?
        .map(|value| value == "1" || value == "true")
        .unwrap_or(false))
}

pub fn set_paused(conn: &Connection, paused: bool) -> CoreResult<()> {
    settings::set(conn, PAUSE_SETTING_KEY, if paused { "1" } else { "0" })
}

pub fn dedup_seconds(conn: &Connection) -> CoreResult<i64> {
    Ok(settings::get(conn, DEDUP_SETTING_KEY)?
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .unwrap_or(DEFAULT_DEDUP_SECONDS))
}

pub fn set_dedup_seconds(conn: &Connection, seconds: i64) -> CoreResult<()> {
    settings::set(conn, DEDUP_SETTING_KEY, &seconds.max(0).to_string())
}

/// 脱敏规则。缺省开启、无自定义词条。
pub fn redaction_rules(conn: &Connection) -> CoreResult<RedactionRules> {
    match settings::get(conn, REDACTION_SETTING_KEY)? {
        Some(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
        None => Ok(RedactionRules::default()),
    }
}

pub fn set_redaction_rules(conn: &Connection, rules: &RedactionRules) -> CoreResult<()> {
    settings::set(
        conn,
        REDACTION_SETTING_KEY,
        &serde_json::to_string(rules).unwrap_or_default(),
    )
}

// ---------- 审计 ----------

pub fn insert_audit(
    conn: &Connection,
    id: &str,
    kind: &str,
    action: &str,
    reason: &str,
) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO capture_audit (id, kind, action, reason, created_at)
         VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        rusqlite::params![id, kind, action, reason],
    )?;
    Ok(())
}

pub fn list_audit(conn: &Connection, limit: i64) -> CoreResult<Vec<CaptureAuditView>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let mut stmt = conn.prepare(
        "SELECT id, kind, action, reason, created_at FROM capture_audit
         ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(CaptureAuditView {
            id: row.get(0)?,
            kind: row.get(1)?,
            action: row.get(2)?,
            reason: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

// ---------- 事件 ----------

/// 去重窗口内是否已有同类型、同哈希的记录。
pub fn hash_seen(
    conn: &Connection,
    kind: &str,
    content_hash: &str,
    occurred_at: &str,
    dedup_seconds: i64,
) -> CoreResult<bool> {
    if content_hash.is_empty() {
        return Ok(false);
    }
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM capture_events
         WHERE kind = ?1 AND content_hash = ?2
           AND occurred_at >= datetime(?3, ?4)",
        rusqlite::params![
            kind,
            content_hash,
            occurred_at,
            format!("-{dedup_seconds} seconds")
        ],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// 写入一条事件与它的派生摘要，调用方保证在同一事务内。
// 参数与 capture_events 的列一一对应，拆成结构体会让调用方多一层无收益的包装。
#[allow(clippy::too_many_arguments)]
pub fn insert_event(
    tx: &Transaction<'_>,
    id: &str,
    kind: &str,
    occurred_at: &str,
    source_app: &str,
    payload_json: &str,
    content_hash: &str,
    redacted: bool,
    created_at: &str,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO capture_events
             (id, kind, occurred_at, source_app, payload_json, content_hash, redacted, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            id,
            kind,
            occurred_at,
            source_app,
            payload_json,
            content_hash,
            i64::from(redacted),
            created_at
        ],
    )?;
    Ok(())
}

pub fn insert_summary(
    tx: &Transaction<'_>,
    id: &str,
    event_id: &str,
    topic: &str,
    excerpt: &str,
    created_at: &str,
) -> CoreResult<()> {
    let excerpt: String = excerpt.chars().take(MAX_EXCERPT_CHARS).collect();
    tx.execute(
        "INSERT INTO capture_summaries (id, event_id, topic, excerpt, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, event_id, topic, excerpt, created_at],
    )?;
    Ok(())
}

fn read_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<CaptureEventView> {
    let payload_json: String = row.get(4)?;
    Ok(CaptureEventView {
        id: row.get(0)?,
        kind: row.get(1)?,
        occurred_at: row.get(2)?,
        source_app: row.get(3)?,
        payload: serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null),
        content_hash: row.get(5)?,
        redacted: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
    })
}

/// 按类型与时间范围分页读取，时间倒序。
pub fn list_events(conn: &Connection, filter: &CaptureFilter) -> CoreResult<Vec<CaptureEventView>> {
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(kind) = filter.kind.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("kind = ?{}", params.len() + 1));
        params.push(Box::new(kind.to_string()));
    }
    if let Some(from) = filter.from.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("occurred_at >= ?{}", params.len() + 1));
        params.push(Box::new(from.to_string()));
    }
    if let Some(to) = filter.to.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("occurred_at <= ?{}", params.len() + 1));
        params.push(Box::new(to.to_string()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT id, kind, occurred_at, source_app, payload_json, content_hash, redacted, created_at
         FROM capture_events {where_clause} ORDER BY occurred_at DESC, rowid DESC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(
        params.iter().map(|param| param.as_ref()),
    ))?;
    let mut events = Vec::new();
    while let Some(row) = rows.next()? {
        events.push(read_event(row)?);
    }
    Ok(events)
}

pub fn summaries_of(conn: &Connection, event_id: &str) -> CoreResult<Vec<CaptureSummaryView>> {
    let mut stmt = conn.prepare(
        "SELECT id, event_id, topic, excerpt, created_at FROM capture_summaries
         WHERE event_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([event_id], |row| {
        Ok(CaptureSummaryView {
            id: row.get(0)?,
            event_id: row.get(1)?,
            topic: row.get(2)?,
            excerpt: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

/// 删除事件及其派生摘要，返回是否确有删除。
pub fn delete_event(conn: &Connection, id: &str) -> CoreResult<bool> {
    let affected = conn.execute("DELETE FROM capture_events WHERE id = ?1", [id])?;
    // 外键级联已在同一语句内处理；这里再显式清理，保证未开启外键时也一致。
    conn.execute("DELETE FROM capture_summaries WHERE event_id = ?1", [id])?;
    Ok(affected > 0)
}

pub fn count_events(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM capture_events", [], |row| row.get(0))?;
    Ok(count)
}
