use rusqlite::Connection;

use std::path::Path;

use crate::error::{CoreError, CoreResult};

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// 迁移按 version 升序追加，已发布的迁移不再修改。
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "masters",
        sql: include_str!("../migrations/0002_masters.sql"),
    },
    Migration {
        version: 3,
        name: "council",
        sql: include_str!("../migrations/0003_council.sql"),
    },
    Migration {
        version: 4,
        name: "network",
        sql: include_str!("../migrations/0004_network.sql"),
    },
    Migration {
        version: 5,
        name: "companion",
        sql: include_str!("../migrations/0005_companion.sql"),
    },
    Migration {
        version: 6,
        name: "distill",
        sql: include_str!("../migrations/0006_distill.sql"),
    },
    Migration {
        version: 7,
        name: "capture",
        sql: include_str!("../migrations/0007_capture.sql"),
    },
    Migration {
        version: 8,
        name: "publish",
        sql: include_str!("../migrations/0008_publish.sql"),
    },
    Migration {
        version: 9,
        name: "assets",
        sql: include_str!("../migrations/0009_assets.sql"),
    },
    Migration {
        version: 10,
        name: "tuning",
        sql: include_str!("../migrations/0010_tuning.sql"),
    },
    Migration {
        version: 11,
        name: "followup",
        sql: include_str!("../migrations/0011_followup.sql"),
    },
    Migration {
        version: 12,
        name: "connectors",
        sql: include_str!("../migrations/0012_connectors.sql"),
    },
    Migration {
        version: 13,
        name: "search_safety",
        sql: include_str!("../migrations/0013_search_safety.sql"),
    },
    Migration {
        version: 14,
        name: "governance",
        sql: include_str!("../migrations/0014_governance.sql"),
    },
    Migration {
        version: 15,
        name: "seat_questions",
        sql: include_str!("../migrations/0015_seat_questions.sql"),
    },
    Migration {
        version: 16,
        name: "layer_pairings",
        sql: include_str!("../migrations/0016_layer_pairings.sql"),
    },
];

fn ensure_ledger(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
         );",
    )?;
    Ok(())
}

/// 代码中定义的最高迁移版本，供测试与界面显示当前目标版本。
pub fn latest_version() -> i64 {
    MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

pub fn current_version(conn: &Connection) -> CoreResult<i64> {
    ensure_ledger(conn)?;
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

/// 按序执行尚未应用的迁移。每个迁移在自己的事务内完成，
/// 失败时该迁移整体回滚，已完成的迁移保持有效。
pub fn apply_all(conn: &mut Connection) -> CoreResult<i64> {
    apply_all_with_dir(conn, None)
}

/// 与 [`apply_all`] 相同，但在版本提升前按设置创建 `pre_migration` 备份。
/// 备份失败时中止迁移，数据库停留在旧版本。
pub fn apply_all_with_dir(conn: &mut Connection, backup_dir: Option<&Path>) -> CoreResult<i64> {
    let mut applied = current_version(conn)?;

    for migration in MIGRATIONS {
        if migration.version <= applied {
            continue;
        }
        let pending_backup = match backup_dir {
            Some(dir) if applied > 0 && crate::backup::before_migration_enabled(conn) => {
                Some(crate::backup::create_file(
                    conn,
                    dir,
                    crate::backup::KIND_PRE_MIGRATION,
                )?)
            }
            _ => None,
        };
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql).map_err(|source| CoreError::Migration {
            version: migration.version,
            name: migration.name.to_string(),
            source,
        })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![migration.version, migration.name],
        )?;
        tx.commit()?;
        applied = migration.version;
        if let Some(outcome) = pending_backup {
            // 账本表可能在本迁移才建立，记录失败不回滚已完成的迁移。
            let _ = crate::backup::record(conn, &outcome, crate::backup::KIND_PRE_MIGRATION);
        }
    }

    Ok(applied)
}

/// 已应用的版本号，按顺序返回，用于校验账本连续性。
pub fn applied_versions(conn: &Connection) -> CoreResult<Vec<i64>> {
    ensure_ledger(conn)?;
    let mut stmt = conn.prepare("SELECT version FROM schema_migrations ORDER BY version ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    let mut versions = Vec::new();
    for row in rows {
        versions.push(row?);
    }
    Ok(versions)
}
