//! P8 数据主权：归档覆盖全部业务表、清除与审计留痕、组合属性。

use std::collections::BTreeMap;

use proptest::prelude::*;
use thought_forge_core::data::service::{self as data_service};
use thought_forge_core::data::{ARCHIVE_FORMAT, DATA_TABLES, SCOPE_ALL};
use thought_forge_core::db::{self, migrations, settings};

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

/// 迁移自带默认行，用于把「本来就有」与「本次写入」区分开。
fn baseline_rows(conn: &rusqlite::Connection) -> i64 {
    DATA_TABLES
        .iter()
        .map(|(table, _)| db::count_rows(conn, table).unwrap())
        .sum()
}

#[test]
fn scope_lists_every_table() {
    let conn = db();
    let scope = data_service::scope(&conn).unwrap();
    assert_eq!(scope.table_count, DATA_TABLES.len() as i64);
    assert_eq!(scope.tables.len(), DATA_TABLES.len());
    assert_eq!(scope.row_count, baseline_rows(&conn));
}

#[test]
fn export_writes_archive_with_all_tables() {
    let conn = db();
    settings::set(&conn, "user.note", "记得备份").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("archive.json");
    let outcome = data_service::export(&conn, &dest).unwrap();

    assert_eq!(outcome.table_count, DATA_TABLES.len() as i64);
    assert!(outcome.bytes > 0);
    assert!(dest.is_file());

    let text = std::fs::read_to_string(&dest).unwrap();
    let document: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(document["format"], ARCHIVE_FORMAT);
    let tables = document["tables"].as_object().unwrap();
    // 归档反映导出前的状态，导出动作本身在此之后才写入审计。
    assert!(tables.contains_key("data_events"));
    let settings_rows = tables["settings"].as_array().unwrap();
    assert!(settings_rows
        .iter()
        .any(|row| row["key"] == "user.note" && row["value"] == "记得备份"));
}

#[test]
fn purge_clears_rows_and_records_event() {
    let mut conn = db();
    settings::set(&conn, "user.note", "记得备份").unwrap();
    let outcome = data_service::purge(&mut conn, SCOPE_ALL).unwrap();
    assert!(outcome.row_count >= 4, "至少清掉设置与迁移默认行");

    let scope = data_service::scope(&conn).unwrap();
    assert_eq!(scope.row_count, 1, "清除后只剩一条清除审计");
    assert_eq!(db::count_rows(&conn, "settings").unwrap(), 0);
    assert_eq!(db::count_rows(&conn, "data_events").unwrap(), 1);

    let events = data_service::events(&conn, 10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "purge");
    assert_eq!(events[0].kind_label, "清除");
}

#[test]
fn purge_rejects_unknown_scope() {
    let mut conn = db();
    let error = data_service::purge(&mut conn, "partial").unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// 导出是只读操作：连续导出不会改变各表行数。
    #[test]
    fn property_export_is_read_only(writes in 0usize..6) {
        let conn = db();
        for index in 0..writes {
            settings::set(&conn, &format!("k{index}"), "v").unwrap();
        }
        let before: BTreeMap<String, i64> = data_service::scope(&conn)
            .unwrap()
            .tables
            .into_iter()
            .map(|table| (table.table, table.rows))
            .collect();
        let dir = tempfile::tempdir().unwrap();
        data_service::export(&conn, &dir.path().join("a.json")).unwrap();
        data_service::export(&conn, &dir.path().join("b.json")).unwrap();
        let after: BTreeMap<String, i64> = data_service::scope(&conn)
            .unwrap()
            .tables
            .into_iter()
            .map(|table| (table.table, table.rows))
            .collect();
        // settings 不变；data_events 因两次导出各增一条，属预期副作用。
        for (table, rows) in &before {
            if table == "data_events" {
                continue;
            }
            prop_assert_eq!(after.get(table), Some(rows));
        }
        let events = data_service::events(&conn, 10).unwrap();
        prop_assert_eq!(events.len(), 2);
        prop_assert!(events.iter().all(|event| event.kind == "export"));
    }
}
