use rusqlite::Connection;

use crate::db::count_rows;
use crate::error::CoreResult;

/// 近七天窗口，用于「新入炉」与会诊频次。
const RECENT_WINDOW_DAYS: i64 = 7;

/// 超过该激活度的节点计为「激活中」。
const ACTIVE_THRESHOLD: f64 = 0.3;

pub struct FurnaceSnapshot {
    pub total_nodes: i64,
    pub active_nodes: i64,
    pub recent_captures: i64,
    pub recent_councils: i64,
    pub computed_at: String,
}

/// 统计各来源计数。相关表在后续迁移中建立，此前一律返回 0，
/// 因此新装应用的炉温自然为冷炉。
pub fn snapshot(conn: &Connection) -> CoreResult<FurnaceSnapshot> {
    let total_nodes = count_rows(conn, "thought_nodes")?;
    let active_nodes = count_active_nodes(conn)?;
    let recent_captures = count_recent(conn, "capture_events")?;
    let recent_councils = count_recent(conn, "council_sessions")?;
    let computed_at: String = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [],
        |row| row.get(0),
    )?;

    Ok(FurnaceSnapshot {
        total_nodes,
        active_nodes,
        recent_captures,
        recent_councils,
        computed_at,
    })
}

fn count_active_nodes(conn: &Connection) -> CoreResult<i64> {
    if !crate::db::table_exists(conn, "thought_nodes")? {
        return Ok(0);
    }
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM thought_nodes
         WHERE activation >= ?1 AND superseded_by IS NULL AND status = 'active'",
        rusqlite::params![ACTIVE_THRESHOLD],
        |row| row.get(0),
    )?;
    Ok(count)
}

fn count_recent(conn: &Connection, table: &str) -> CoreResult<i64> {
    if !crate::db::table_exists(conn, table)? {
        return Ok(0);
    }
    let sql = format!(
        "SELECT COUNT(*) FROM {table} WHERE created_at >= datetime('now', '-{RECENT_WINDOW_DAYS} days')"
    );
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}
