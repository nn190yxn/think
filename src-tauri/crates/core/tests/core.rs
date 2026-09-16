use thought_forge_core::db::{self, migrations, settings};
use thought_forge_core::furnace;

fn migrated_memory_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

#[test]
fn migration_is_idempotent_and_records_versions() {
    let mut conn = migrated_memory_db();
    let latest = migrations::latest_version();

    assert_eq!(migrations::current_version(&conn).unwrap(), latest);
    assert_eq!(
        migrations::applied_versions(&conn).unwrap(),
        (1..=latest).collect::<Vec<_>>()
    );

    // 重复执行不应重复记账，也不应报错。
    let again = migrations::apply_all(&mut conn).unwrap();
    assert_eq!(again, latest);
    assert_eq!(
        migrations::applied_versions(&conn).unwrap(),
        (1..=latest).collect::<Vec<_>>()
    );
}

#[test]
fn connection_pragmas_are_applied() {
    let conn = migrated_memory_db();
    assert!(db::foreign_keys_enabled(&conn).unwrap());
    assert!(db::table_exists(&conn, "settings").unwrap());
    assert!(!db::table_exists(&conn, "not_a_table").unwrap());
}

#[test]
fn settings_round_trip_and_overwrite() {
    let conn = migrated_memory_db();

    assert_eq!(settings::get(&conn, "theme").unwrap(), None);
    settings::set(&conn, "theme", "kiln").unwrap();
    assert_eq!(settings::get(&conn, "theme").unwrap().as_deref(), Some("kiln"));

    settings::set(&conn, "theme", "suci").unwrap();
    assert_eq!(settings::get(&conn, "theme").unwrap().as_deref(), Some("suci"));
}

#[test]
fn settings_rejects_invalid_input() {
    let conn = migrated_memory_db();
    assert!(settings::set(&conn, "  ", "value").is_err());
    assert!(settings::set(&conn, "theme", &"x".repeat(5000)).is_err());
}

#[test]
fn furnace_snapshot_is_cold_on_fresh_database() {
    let conn = migrated_memory_db();
    let snapshot = furnace::snapshot(&conn).unwrap();

    assert_eq!(snapshot.total_nodes, 0);
    assert_eq!(snapshot.active_nodes, 0);
    assert_eq!(snapshot.recent_captures, 0);
    assert_eq!(snapshot.recent_councils, 0);
    assert!(snapshot.computed_at.ends_with('Z'));
}

#[test]
fn app_info_reports_schema_version() {
    let conn = migrated_memory_db();
    let info = thought_forge_core::app_info(&conn).unwrap();
    assert_eq!(info.name, "思想熔炉");
    assert_eq!(info.schema_version, migrations::latest_version());
    assert!(!info.version.is_empty());
}

#[cfg(unix)]
#[test]
fn file_database_uses_wal_and_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data").join("forge.db");

    {
        let (conn, version) = db::initialize(&path).unwrap();
        assert_eq!(version, migrations::latest_version());
        assert_eq!(db::journal_mode(&conn).unwrap().to_lowercase(), "wal");
        settings::set(&conn, "theme", "suci").unwrap();
    }

    // 重新打开：迁移不重复执行，数据仍在。
    let (conn, version) = db::initialize(&path).unwrap();
    assert_eq!(version, migrations::latest_version());
    assert_eq!(settings::get(&conn, "theme").unwrap().as_deref(), Some("suci"));
}
