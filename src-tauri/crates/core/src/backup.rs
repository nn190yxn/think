//! 备份与恢复：用 `VACUUM INTO` 产出一致性快照，恢复前先完整性校验。
//!
//! 恢复本身由外壳在关闭连接后替换数据文件，内核只做校验与准备，
//! 避免在写事务中替换自身。

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

pub const KIND_MANUAL: &str = "manual";
pub const KIND_PRE_MIGRATION: &str = "pre_migration";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOutcome {
    pub path: String,
    pub size_bytes: i64,
    pub checksum: String,
    pub schema_version: i64,
    pub kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupView {
    pub id: String,
    pub path: String,
    pub size_bytes: i64,
    pub checksum: String,
    pub schema_version: i64,
    pub kind: String,
    pub present: bool,
    pub created_at: String,
}

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

fn checksum_of(path: &Path) -> CoreResult<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn size_of(path: &Path) -> CoreResult<i64> {
    Ok(std::fs::metadata(path)?.len() as i64)
}

/// 打开备份文件并读回其中的架构版本与完整性结论。
fn inspect(path: &Path) -> CoreResult<(i64, String)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(CoreError::InvalidInput(format!(
            "备份完整性校验未通过：{integrity}"
        )));
    }
    let schema_version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    Ok((schema_version, integrity))
}

fn file_stamp(created_at: &str) -> String {
    created_at
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

/// 只创建备份文件，不写 `backups` 表。迁移前备份会用到它。
pub fn create_file(conn: &Connection, dir: &Path, kind: &str) -> CoreResult<BackupOutcome> {
    std::fs::create_dir_all(dir)?;
    let created_at = now(conn)?;
    let path = dir.join(format!(
        "thought-forge-{kind}-{}.sqlite3",
        file_stamp(&created_at)
    ));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let target = path.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", [target])?;
    verify(&path)
}

/// 校验一个备份文件的大小、摘要、完整性与架构版本。
pub fn verify(path: &Path) -> CoreResult<BackupOutcome> {
    if !path.exists() {
        return Err(CoreError::NotFound(format!(
            "备份文件 {}",
            path.display()
        )));
    }
    let (schema_version, _) = inspect(path)?;
    let conn = Connection::open(path)?;
    let created_at = now(&conn).unwrap_or_default();
    Ok(BackupOutcome {
        path: path.to_string_lossy().to_string(),
        size_bytes: size_of(path)?,
        checksum: checksum_of(path)?,
        schema_version,
        kind: String::new(),
        created_at,
    })
}

/// 把备份写入账本。`backups` 表不存在时跳过，保证早期迁移也能自动备份。
pub fn record(conn: &Connection, outcome: &BackupOutcome, kind: &str) -> CoreResult<Option<String>> {
    if !crate::db::table_exists(conn, "backups")? {
        return Ok(None);
    }
    let id = unique_id("backup", &outcome.path);
    conn.execute(
        "INSERT INTO backups
             (id, path, size_bytes, checksum, schema_version, kind, present, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
        rusqlite::params![
            id,
            outcome.path,
            outcome.size_bytes,
            outcome.checksum,
            outcome.schema_version,
            kind,
            if outcome.created_at.is_empty() {
                now(conn)?
            } else {
                outcome.created_at.clone()
            },
        ],
    )?;
    Ok(Some(id))
}

/// 创建备份并写入账本。
pub fn create(conn: &Connection, dir: &Path, kind: &str) -> CoreResult<BackupOutcome> {
    let mut outcome = create_file(conn, dir, kind)?;
    outcome.kind = kind.to_string();
    record(conn, &outcome, kind)?;
    Ok(outcome)
}

/// 备份列表，按时间倒序。
pub fn list(conn: &Connection, limit: i64) -> CoreResult<Vec<BackupView>> {
    if !crate::db::table_exists(conn, "backups")? {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT id, path, size_bytes, checksum, schema_version, kind, present, created_at
         FROM backups ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(BackupView {
            id: row.get(0)?,
            path: row.get(1)?,
            size_bytes: row.get(2)?,
            checksum: row.get(3)?,
            schema_version: row.get(4)?,
            kind: row.get(5)?,
            present: row.get::<_, i64>(6)? != 0,
            created_at: row.get(7)?,
        })
    })?;
    let mut list = Vec::new();
    for row in rows {
        list.push(row?);
    }
    Ok(list)
}

/// 按保留份数裁剪：超出部分删除文件并把 `present` 置假，返回被移除的份数。
pub fn prune(conn: &Connection, keep: i64) -> CoreResult<i64> {
    if !crate::db::table_exists(conn, "backups")? {
        return Ok(0);
    }
    let keep = keep.max(1);
    let stale: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, path FROM backups
             WHERE present = 1
             ORDER BY created_at DESC, rowid DESC
             LIMIT -1 OFFSET ?1",
        )?;
        let rows = stmt.query_map([keep], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        list
    };
    let mut removed = 0i64;
    for (id, path) in stale {
        let path = Path::new(&path);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        conn.execute("UPDATE backups SET present = 0 WHERE id = ?1", [id])?;
        removed += 1;
    }
    Ok(removed)
}

/// 恢复准备：先看文件本身，再与账本里登记时的摘要核对，两关都过才允许替换。只看文件本身挡不住篡改：改坏的库多数仍能被 SQLite 打开；账本里查不到的路径也一律拒绝。
pub fn restore_prepare(conn: &Connection, path: &Path) -> CoreResult<BackupOutcome> {
    let outcome = verify(path)?;
    if !crate::db::table_exists(conn, "backups")? {
        return Err(CoreError::InvalidInput(
            "备份账本不存在，拒绝恢复".to_string(),
        ));
    }
    let recorded = {
        let mut stmt = conn.prepare(
            "SELECT checksum, size_bytes FROM backups
             WHERE path = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([&outcome.path])?;
        match rows.next()? {
            Some(row) => Some((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            None => None,
        }
    };
    let Some((checksum, size_bytes)) = recorded else {
        return Err(CoreError::InvalidInput(format!(
            "{} 不在备份账本里，拒绝恢复",
            outcome.path
        )));
    };
    if checksum != outcome.checksum {
        let head = |value: &str| value[..value.len().min(12)].to_string();
        return Err(CoreError::InvalidInput(format!(
            "备份摘要与登记时不一致（登记 {}，现在是 {}），文件可能被改过，拒绝恢复",
            head(&checksum),
            head(&outcome.checksum)
        )));
    }
    if size_bytes != outcome.size_bytes {
        return Err(CoreError::InvalidInput(format!(
            "备份大小与登记时不一致（登记 {size_bytes} 字节，现在是 {} 字节），拒绝恢复",
            outcome.size_bytes
        )));
    }
    Ok(outcome)
}

/// 迁移前是否需要自动备份。设置缺失或不可解析时按需要处理。
pub fn before_migration_enabled(conn: &Connection) -> bool {
    crate::council::tuning::bool_of(conn, "backup.before_migration").unwrap_or(true)
}

/// 按保留天数清理超期的网页正文快照，来源元数据行保留。
pub fn prune_snapshots(conn: &Connection, retain_days: i64) -> CoreResult<i64> {
    if !crate::db::table_exists(conn, "council_sources")? {
        return Ok(0);
    }
    let retain_days = retain_days.max(1);
    let affected = conn.execute(
        "UPDATE council_sources SET body = NULL
         WHERE body IS NOT NULL
           AND created_at < datetime('now', ?1)",
        [format!("-{retain_days} days")],
    )?;
    Ok(affected as i64)
}
