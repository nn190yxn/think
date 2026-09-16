//! 自我蒸馏仓储：草稿、候选条目、席位开关与历史记录读取。

use rusqlite::Connection;

use crate::db::settings;
use crate::error::CoreResult;
use crate::master::Layer;
use crate::util::unique_id;

use super::{
    SelfDraftState, SelfDraftView, SelfItemState, SelfItemView, SelfRecord,
    SEAT_SETTING_KEY, SELF_MASTER_ID,
};

const MAX_LIMIT: i64 = 200;

pub fn now(conn: &Connection) -> CoreResult<String> {
    let value: String = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [],
        |row| row.get(0),
    )?;
    Ok(value)
}

/// 可用于自我蒸馏的思考记录条数。
pub fn record_count(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM thought_records", [], |row| row.get(0))?;
    Ok(count)
}

/// 读取最近的思考记录，作为自我蒸馏的材料。
pub fn records(conn: &Connection, limit: i64) -> CoreResult<Vec<SelfRecord>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let mut stmt = conn.prepare(
        "SELECT id, question, conclusion, adopted, reason, domains_json, layers_json, created_at
           FROM thought_records
          ORDER BY created_at DESC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(SelfRecord {
            id: row.get(0)?,
            question: row.get(1)?,
            conclusion: row.get(2)?,
            adopted: row.get::<_, i64>(3)? != 0,
            reason: row.get(4)?,
            domains: parse_strings(&row.get::<_, String>(5)?),
            layers: parse_strings(&row.get::<_, String>(6)?)
                .iter()
                .filter_map(|name| Layer::parse(name))
                .collect(),
            created_at: row.get(7)?,
        })
    })?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

/// 已安装的「你」的当前版本号，未安装时为 0。
pub fn installed_version(conn: &Connection) -> CoreResult<i64> {
    let version: Option<i64> = conn
        .query_row(
            "SELECT current_version FROM masters WHERE id = ?1",
            [SELF_MASTER_ID],
            |row| row.get(0),
        )
        .ok();
    Ok(version.unwrap_or(0))
}

pub fn seat_enabled(conn: &Connection) -> CoreResult<bool> {
    Ok(settings::get(conn, SEAT_SETTING_KEY)?
        .map(|value| value == "1" || value == "true")
        .unwrap_or(false))
}

pub fn set_seat_enabled(conn: &Connection, enabled: bool) -> CoreResult<()> {
    settings::set(conn, SEAT_SETTING_KEY, if enabled { "1" } else { "0" })
}

// ---------- 草稿 ----------

pub fn create_draft(conn: &Connection, record_count: i64) -> CoreResult<SelfDraftView> {
    let id = unique_id("self", &format!("{record_count}-{}", now(conn)?));
    let created_at = now(conn)?;
    conn.execute(
        "INSERT INTO self_drafts
             (id, status, record_count, master_id, model_calls, note, updated_at, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, '', ?5, ?5)",
        rusqlite::params![
            id,
            SelfDraftState::Ready.as_str(),
            record_count,
            SELF_MASTER_ID,
            created_at
        ],
    )?;
    get_draft(conn, &id)
}

pub fn get_draft(conn: &Connection, draft_id: &str) -> CoreResult<SelfDraftView> {
    let draft = conn.query_row(
        "SELECT id, status, record_count, master_id, model_calls, error_code, note,
                updated_at, created_at
           FROM self_drafts WHERE id = ?1",
        [draft_id],
        map_draft_row,
    )?;
    Ok(draft)
}

pub fn latest_draft(conn: &Connection) -> CoreResult<Option<SelfDraftView>> {
    let mut stmt = conn.prepare(
        "SELECT id, status, record_count, master_id, model_calls, error_code, note,
                updated_at, created_at
           FROM self_drafts ORDER BY created_at DESC LIMIT 1",
    )?;
    let mut rows = stmt.query([])?;
    match rows.next()? {
        Some(row) => Ok(Some(map_draft_row(row)?)),
        None => Ok(None),
    }
}

pub fn list_drafts(conn: &Connection, limit: i64) -> CoreResult<Vec<SelfDraftView>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let mut stmt = conn.prepare(
        "SELECT id, status, record_count, master_id, model_calls, error_code, note,
                updated_at, created_at
           FROM self_drafts ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], map_draft_row)?;
    let mut drafts = Vec::new();
    for row in rows {
        drafts.push(row?);
    }
    Ok(drafts)
}

pub fn set_draft_state(
    conn: &Connection,
    draft_id: &str,
    state: SelfDraftState,
    error_code: Option<&str>,
    note: &str,
    model_calls: i64,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE self_drafts
            SET status = ?2, error_code = ?3, note = ?4, model_calls = ?5, updated_at = ?6
          WHERE id = ?1",
        rusqlite::params![
            draft_id,
            state.as_str(),
            error_code,
            note,
            model_calls,
            now(conn)?
        ],
    )?;
    Ok(())
}

// ---------- 候选条目 ----------

// 参数与自我蒸馏候选条目的列一一对应，拆成结构体会让调用方多一层无收益的包装。
#[allow(clippy::too_many_arguments)]
pub fn insert_item(
    conn: &Connection,
    draft_id: &str,
    ordinal: i64,
    title: &str,
    layer: Layer,
    trigger_condition: &str,
    steps: &[String],
    mechanism: &str,
    boundary: &str,
    evidence: &[String],
    source_record_id: Option<&str>,
) -> CoreResult<String> {
    let id = unique_id("selfitem", &format!("{draft_id}-{ordinal}-{title}"));
    conn.execute(
        "INSERT INTO self_items
             (id, draft_id, ordinal, title, layer, trigger_condition, steps_json, mechanism,
              boundary, evidence_json, source_record_id, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            id,
            draft_id,
            ordinal,
            title,
            layer.as_str(),
            trigger_condition,
            serde_json::to_string(steps).unwrap_or_else(|_| "[]".to_string()),
            mechanism,
            boundary,
            serde_json::to_string(evidence).unwrap_or_else(|_| "[]".to_string()),
            source_record_id,
            SelfItemState::Pending.as_str(),
            now(conn)?,
        ],
    )?;
    Ok(id)
}

pub fn items(conn: &Connection, draft_id: &str) -> CoreResult<Vec<SelfItemView>> {
    let mut stmt = conn.prepare(
        "SELECT id, draft_id, ordinal, title, layer, trigger_condition, steps_json, mechanism,
                boundary, evidence_json, source_record_id, status, created_at
           FROM self_items WHERE draft_id = ?1 ORDER BY ordinal ASC",
    )?;
    let rows = stmt.query_map([draft_id], map_item_row)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

pub fn decide_item(conn: &Connection, item_id: &str, accepted: bool) -> CoreResult<()> {
    let state = if accepted {
        SelfItemState::Accepted
    } else {
        SelfItemState::Rejected
    };
    let changed = conn.execute(
        "UPDATE self_items SET status = ?2 WHERE id = ?1",
        rusqlite::params![item_id, state.as_str()],
    )?;
    if changed == 0 {
        return Err(crate::error::CoreError::NotFound(format!(
            "候选条目不存在：{item_id}"
        )));
    }
    Ok(())
}

/// 返回 (待确认, 已采纳, 已剔除) 三个计数。
pub fn item_counts(conn: &Connection, draft_id: &str) -> CoreResult<(i64, i64, i64)> {
    let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM self_items WHERE draft_id = ?1 GROUP BY status")?;
    let rows = stmt.query_map([draft_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut pending = 0i64;
    let mut accepted = 0i64;
    let mut rejected = 0i64;
    for row in rows {
        let (status, count) = row?;
        match status.as_str() {
            "accepted" => accepted += count,
            "rejected" => rejected += count,
            _ => pending += count,
        }
    }
    Ok((pending, accepted, rejected))
}

/// 只读取已采纳的条目，供安装使用。
pub fn accepted_items(conn: &Connection, draft_id: &str) -> CoreResult<Vec<SelfItemView>> {
    Ok(items(conn, draft_id)?
        .into_iter()
        .filter(|item| item.status == SelfItemState::Accepted.as_str())
        .collect())
}

fn map_draft_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SelfDraftView> {
    Ok(SelfDraftView {
        id: row.get(0)?,
        status: row.get(1)?,
        record_count: row.get(2)?,
        master_id: row.get(3)?,
        model_calls: row.get(4)?,
        error_code: row.get(5)?,
        note: row.get(6)?,
        updated_at: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn map_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SelfItemView> {
    let layer_raw: String = row.get(4)?;
    let layer = Layer::parse(&layer_raw).unwrap_or(Layer::Dao);
    let status: String = row.get(11)?;
    let status_label = match status.as_str() {
        "accepted" => SelfItemState::Accepted.name(),
        "rejected" => SelfItemState::Rejected.name(),
        _ => SelfItemState::Pending.name(),
    };
    Ok(SelfItemView {
        id: row.get(0)?,
        draft_id: row.get(1)?,
        ordinal: row.get(2)?,
        title: row.get(3)?,
        layer,
        trigger_condition: row.get(5)?,
        steps: parse_strings(&row.get::<_, String>(6)?),
        mechanism: row.get(7)?,
        boundary: row.get(8)?,
        evidence: parse_strings(&row.get::<_, String>(9)?),
        source_record_id: row.get(10)?,
        status,
        status_label: status_label.to_string(),
        created_at: row.get(12)?,
    })
}

fn parse_strings(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}
