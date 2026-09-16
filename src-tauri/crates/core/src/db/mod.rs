use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::CoreResult;

pub mod migrations;
pub mod settings;

/// 连接级参数。外键必须在事务外开启，否则 SQLite 会静默忽略。
pub fn configure(conn: &Connection) -> CoreResult<()> {
    conn.busy_timeout(Duration::from_millis(5_000))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;",
    )?;
    Ok(())
}

/// 打开文件库并把日志模式切到 WAL，兼顾读写并发与崩溃安全。
pub fn open(path: impl AsRef<Path>) -> CoreResult<Connection> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&conn)?;
    Ok(conn)
}

/// 内存库用于测试与只读演示，跳过 WAL。
pub fn open_in_memory() -> CoreResult<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    Ok(conn)
}

/// 打开、配置并迁移到位，返回可直接使用的连接与当前版本。
pub fn initialize(path: impl AsRef<Path>) -> CoreResult<(Connection, i64)> {
    let mut conn = open(path)?;
    let version = migrations::apply_all(&mut conn)?;
    Ok((conn, version))
}

pub fn journal_mode(conn: &Connection) -> CoreResult<String> {
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    Ok(mode)
}

pub fn foreign_keys_enabled(conn: &Connection) -> CoreResult<bool> {
    let enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    Ok(enabled == 1)
}

pub fn table_exists(conn: &Connection, table: &str) -> CoreResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        rusqlite::params![table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// 表尚未建立的阶段返回 0，让统计接口在任意迁移进度下都可用。
pub fn count_rows(conn: &Connection, table: &str) -> CoreResult<i64> {
    if !table_exists(conn, table)? {
        return Ok(0);
    }
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}

/// 带时间窗的计数。列名固定为 `occurred_at` 或 `created_at`。
pub fn count_since_days(conn: &Connection, table: &str, days: i64) -> CoreResult<i64> {
    if !table_exists(conn, table)? {
        return Ok(0);
    }
    let sql = format!(
        "SELECT COUNT(*) FROM {table} WHERE created_at >= datetime('now', '-{days} days')"
    );
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}
