//! 主动助学的设置与洞察仓储。

use rusqlite::{Connection, OptionalExtension};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

use super::{
    CompanionRules, CompanionSettings, Insight, InsightFilter, InsightKind, NewInsight,
    DEFAULT_DAILY_LIMIT, MAX_DAILY_LIMIT, SOURCE_COMPANION,
};

const DEFAULT_ID: &str = "default";

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

fn parse_rules(raw: &str) -> CompanionRules {
    serde_json::from_str(raw).unwrap_or_default()
}

fn parse_strings(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn json_strings(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn ensure_row(conn: &Connection) -> CoreResult<()> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM companion_settings WHERE id = ?1",
        [DEFAULT_ID],
        |row| row.get(0),
    )?;
    if exists == 0 {
        conn.execute(
            "INSERT INTO companion_settings (id, enabled, daily_limit, rules_json, updated_at)
             VALUES (?1, 0, ?2, '{}', ?3)",
            rusqlite::params![DEFAULT_ID, DEFAULT_DAILY_LIMIT, now(conn)?],
        )?;
    }
    Ok(())
}

/// 当日（UTC）由助理推送的洞察数，用于每日上限判定。
pub fn used_today(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM insights
         WHERE source = ?1 AND date(created_at) = date('now')",
        [SOURCE_COMPANION],
        |row| row.get(0),
    )?;
    Ok(count)
}

fn read_settings(conn: &Connection) -> CoreResult<CompanionSettings> {
    let row: (i64, i64, String, String) = conn.query_row(
        "SELECT enabled, daily_limit, rules_json, updated_at FROM companion_settings WHERE id = ?1",
        [DEFAULT_ID],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let (enabled, daily_limit, rules_json, updated_at) = row;
    Ok(CompanionSettings {
        enabled: enabled != 0,
        daily_limit,
        rules: parse_rules(&rules_json),
        used_today: used_today(conn)?,
        updated_at,
    })
}

pub fn get_settings(conn: &Connection) -> CoreResult<CompanionSettings> {
    ensure_row(conn)?;
    read_settings(conn)
}

pub fn set_enabled(conn: &Connection, enabled: bool) -> CoreResult<CompanionSettings> {
    ensure_row(conn)?;
    conn.execute(
        "UPDATE companion_settings SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![DEFAULT_ID, i64::from(enabled), now(conn)?],
    )?;
    read_settings(conn)
}

pub fn set_daily_limit(conn: &Connection, limit: i64) -> CoreResult<CompanionSettings> {
    if !(1..=MAX_DAILY_LIMIT).contains(&limit) {
        return Err(CoreError::InvalidInput(format!(
            "每日推送上限需在 1 到 {MAX_DAILY_LIMIT} 之间"
        )));
    }
    ensure_row(conn)?;
    conn.execute(
        "UPDATE companion_settings SET daily_limit = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![DEFAULT_ID, limit, now(conn)?],
    )?;
    read_settings(conn)
}

pub fn set_rules(conn: &Connection, rules: &CompanionRules) -> CoreResult<CompanionSettings> {
    ensure_row(conn)?;
    let payload = serde_json::to_string(rules)
        .map_err(|source| CoreError::InvalidInput(format!("规则无法序列化：{source}")))?;
    conn.execute(
        "UPDATE companion_settings SET rules_json = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![DEFAULT_ID, payload, now(conn)?],
    )?;
    read_settings(conn)
}

/// 当日剩余推送额度。
pub fn remaining_today(conn: &Connection, settings: &CompanionSettings) -> CoreResult<i64> {
    let used = used_today(conn)?;
    Ok((settings.daily_limit - used).max(0))
}

fn map_insight(row: &rusqlite::Row<'_>) -> rusqlite::Result<Insight> {
    let kind_raw: String = row.get(1)?;
    Ok(Insight {
        id: row.get(0)?,
        kind: InsightKind::parse(&kind_raw).unwrap_or(InsightKind::Relation),
        title: row.get(2)?,
        summary: row.get(3)?,
        related_node_ids: parse_strings(&row.get::<_, String>(4)?),
        related_master_ids: parse_strings(&row.get::<_, String>(5)?),
        evidence: parse_strings(&row.get::<_, String>(6)?),
        status: row.get(7)?,
        action: row.get(8)?,
        reason: row.get(9)?,
        source: row.get(10)?,
        created_at: row.get(11)?,
    })
}

const INSIGHT_SELECT: &str = "SELECT id, kind, title, summary, related_node_ids_json,
        related_master_ids_json, evidence_json, status, action, reason, source, created_at
     FROM insights";

/// 直接写入一条洞察，不做推送上限判定。
pub fn insert_insight(conn: &Connection, input: &NewInsight<'_>) -> CoreResult<Insight> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(CoreError::InvalidInput("洞察标题不能为空".into()));
    }
    let created_at = now(conn)?;
    let id = unique_id(
        "insight",
        &format!("{}:{}:{}", input.kind.as_str(), title, created_at),
    );
    conn.execute(
        "INSERT INTO insights
             (id, kind, title, summary, related_node_ids_json, related_master_ids_json,
              evidence_json, status, action, reason, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'new', '', '', ?8, ?9)",
        rusqlite::params![
            id,
            input.kind.as_str(),
            title,
            input.summary.trim(),
            json_strings(input.related_node_ids),
            json_strings(input.related_master_ids),
            json_strings(input.evidence),
            input.source,
            created_at,
        ],
    )?;
    get_insight(conn, &id)
}

/// 助理推送洞察。达到当日上限时返回 `None`，不写入。
///
/// 只有 `SOURCE_COMPANION` 来源计入上限，用户手动触发的固化识别不受限。
pub fn push_insight(
    conn: &Connection,
    settings: &CompanionSettings,
    input: &NewInsight<'_>,
) -> CoreResult<Option<Insight>> {
    if input.source == SOURCE_COMPANION && remaining_today(conn, settings)? <= 0 {
        return Ok(None);
    }
    insert_insight(conn, input).map(Some)
}

pub fn get_insight(conn: &Connection, insight_id: &str) -> CoreResult<Insight> {
    conn.query_row(
        &format!("{INSIGHT_SELECT} WHERE id = ?1"),
        [insight_id],
        map_insight,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("洞察 {insight_id}")))
}

/// 按类型、状态与来源过滤洞察，时间倒序。
pub fn list_insights(
    conn: &Connection,
    filter: &InsightFilter,
    limit: i64,
) -> CoreResult<Vec<Insight>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn.prepare(&format!(
        "{INSIGHT_SELECT} ORDER BY created_at DESC, rowid DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit], map_insight)?;
    let mut insights = Vec::new();
    for row in rows {
        insights.push(row?);
    }
    insights.retain(|insight| {
        filter.kind.is_none_or(|kind| insight.kind == kind)
            && filter
                .status
                .as_deref()
                .is_none_or(|status| insight.status == status)
            && filter
                .source
                .as_deref()
                .is_none_or(|source| insight.source == source)
    });
    Ok(insights)
}

/// 处置一条洞察：采纳、忽略或转为会诊。
pub fn mark_insight(
    conn: &Connection,
    insight_id: &str,
    action: &str,
    reason: &str,
) -> CoreResult<Insight> {
    let (action, status) = match action.trim().to_ascii_lowercase().as_str() {
        "adopt" | "采纳" => ("adopt", "adopted"),
        "ignore" | "忽略" => ("ignore", "ignored"),
        "convert" | "转会诊" | "转为会诊" => ("convert", "converted"),
        other => {
            return Err(CoreError::InvalidInput(format!(
                "未知洞察处置：{other}，只支持 adopt/ignore/convert"
            )))
        }
    };
    let affected = conn.execute(
        "UPDATE insights SET action = ?2, status = ?3, reason = ?4 WHERE id = ?1",
        rusqlite::params![insight_id, action, status, reason.trim()],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("洞察 {insight_id}")));
    }
    get_insight(conn, insight_id)
}

/// 尚未处置的洞察数，供余烬入口展示。
pub fn pending_count(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM insights WHERE status = 'new'",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}
