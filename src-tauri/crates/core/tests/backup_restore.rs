//! 恢复前校验：改过的备份、账本外的文件都要被拒；完好的备份才放行。
//!
//! 这条防线之前只有「能当数据库打开」一关，改坏的库多数仍能打开，于是坏文件会被
//! 原样盖上正式库。这里把两关都钉住。

use std::path::PathBuf;

use tempfile::TempDir;

use thought_forge_core::db;
use thought_forge_core::backup;

fn fresh_db(dir: &TempDir) -> (rusqlite::Connection, PathBuf) {
    let path = dir.path().join("forge.db");
    let (conn, _) = db::initialize(&path).expect("库可初始化");
    (conn, path)
}

#[test]
fn intact_backup_passes_and_tampered_one_is_rejected() {
    let dir = TempDir::new().unwrap();
    let (conn, _) = fresh_db(&dir);
    let backups = dir.path().join("backups");

    let created = backup::create(&conn, &backups, backup::KIND_MANUAL).expect("可建备份");
    let path = PathBuf::from(&created.path);
    let good = std::fs::read(&path).unwrap();

    let passed = backup::restore_prepare(&conn, &path).expect("完好备份应放行");
    assert_eq!(passed.checksum, created.checksum, "放行时摘要不变");

    let mut bytes = good.clone();
    let middle = bytes.len() / 2;
    bytes[middle] = bytes[middle].wrapping_add(0x5a);
    std::fs::write(&path, &bytes).unwrap();

    let refused = backup::restore_prepare(&conn, &path);
    assert!(refused.is_err(), "改过的备份必须被拒");
    let message = refused.unwrap_err().to_string();
    assert!(
        message.contains("摘要") || message.contains("大小"),
        "拒绝原因要说清是摘要或大小对不上，实际：{message}"
    );

    std::fs::write(&path, &good).unwrap();
    assert!(
        backup::restore_prepare(&conn, &path).is_ok(),
        "放回原文件后应重新放行"
    );
}

#[test]
fn backup_outside_the_ledger_is_rejected() {
    let dir = TempDir::new().unwrap();
    let (conn, _) = fresh_db(&dir);
    let backups = dir.path().join("backups");

    let created = backup::create(&conn, &backups, backup::KIND_MANUAL).expect("可建备份");
    let moved = dir.path().join("elsewhere.sqlite3");
    std::fs::copy(&created.path, &moved).unwrap();

    let refused = backup::restore_prepare(&conn, &moved);
    assert!(refused.is_err(), "账本里没有的路径必须被拒");
    assert!(
        refused.unwrap_err().to_string().contains("不在备份账本里"),
        "拒绝原因要说明账本里没有这条"
    );
}
