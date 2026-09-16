//! 备份与恢复：创建、校验、恢复前拦截、保留份数裁剪与正文快照清理。

use rusqlite::Connection;
use tempfile::TempDir;
use thought_forge_core::backup;
use thought_forge_core::council::{repo as council_repo, Strategy};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::master::LAYER_ORDER;

fn db() -> Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

#[test]
fn create_writes_file_and_ledger() {
    let conn = db();
    let dir = TempDir::new().unwrap();
    let outcome = backup::create(&conn, dir.path(), backup::KIND_MANUAL).unwrap();

    assert!(std::path::Path::new(&outcome.path).exists());
    assert!(outcome.size_bytes > 0);
    assert!(!outcome.checksum.is_empty());
    assert_eq!(outcome.schema_version, migrations::latest_version());
    assert_eq!(outcome.kind, backup::KIND_MANUAL);

    let list = backup::list(&conn, 10).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].kind, backup::KIND_MANUAL);
    assert!(list[0].present);
}

#[test]
fn verify_rejects_corrupted_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("broken.sqlite3");
    std::fs::write(&path, b"this is not a sqlite database").unwrap();

    let error = backup::restore_prepare(&path).unwrap_err();
    assert!(
        error.code() == "E_DB" || error.code() == "E_INVALID_INPUT",
        "损坏文件应被拒绝，实际为 {}",
        error.code()
    );
}

#[test]
fn prune_keeps_recent_and_marks_removed() {
    let conn = db();
    let dir = TempDir::new().unwrap();
    for (index, created_at) in [
        "2026-09-15T08:00:00Z",
        "2026-09-14T08:00:00Z",
        "2026-09-13T08:00:00Z",
        "2026-09-12T08:00:00Z",
    ]
    .iter()
    .enumerate()
    {
        let path = dir.path().join(format!("snapshot-{index}.sqlite3"));
        std::fs::write(&path, b"stub").unwrap();
        conn.execute(
            "INSERT INTO backups
                 (id, path, size_bytes, checksum, schema_version, kind, present, created_at)
             VALUES (?1, ?2, 4, 'stub', 14, 'manual', 1, ?3)",
            rusqlite::params![format!("backup-{index}"), path.to_string_lossy(), created_at],
        )
        .unwrap();
    }

    let removed = backup::prune(&conn, 2).unwrap();
    assert_eq!(removed, 2);
    let present: Vec<String> = backup::list(&conn, 10)
        .unwrap()
        .into_iter()
        .filter(|item| item.present)
        .map(|item| item.created_at)
        .collect();
    assert_eq!(present, vec!["2026-09-15T08:00:00Z", "2026-09-14T08:00:00Z"]);
}

#[test]
fn migration_bump_creates_pre_migration_backup() {
    let mut conn = db::open_in_memory().unwrap();
    // 手工推进到上一个版本，让下一次提升必然触发迁移前备份。
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
         );",
    )
    .unwrap();
    for migration in migrations::MIGRATIONS {
        if migration.version >= migrations::latest_version() {
            continue;
        }
        conn.execute_batch(migration.sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![migration.version, migration.name],
        )
        .unwrap();
    }

    let dir = TempDir::new().unwrap();
    let version = migrations::apply_all_with_dir(&mut conn, Some(dir.path())).unwrap();
    assert_eq!(version, migrations::latest_version());

    let created: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        created
            .iter()
            .any(|name| name.contains(backup::KIND_PRE_MIGRATION)),
        "迁移前应生成 pre_migration 备份，实际：{created:?}"
    );
}

#[test]
fn snapshot_prune_clears_body_and_keeps_metadata() {
    let conn = db();
    let session_id = council_repo::create_session(
        &conn,
        "要不要换赛道",
        &[],
        &LAYER_ORDER,
        Strategy::Steady,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO council_sources
             (id, session_id, panel_rotation, round, kind, title, url, snippet,
              fetched_at, body, flagged, created_at)
         VALUES ('src-old', ?1, 1, 0, 'search', '旧资料', 'https://example.com',
                 '摘要', '2020-01-01T00:00:00Z', '正文', 0, '2020-01-01T00:00:00Z')",
        [&session_id],
    )
    .unwrap();

    let cleared = backup::prune_snapshots(&conn, 30).unwrap();
    assert_eq!(cleared, 1);
    let body: Option<String> = conn
        .query_row("SELECT body FROM council_sources WHERE id = 'src-old'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(body.is_none(), "超期正文应被清空");
    let title: String = conn
        .query_row("SELECT title FROM council_sources WHERE id = 'src-old'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(title, "旧资料", "元数据行应保留");
}
