//! 真机备份探针：验「坏备份被拒」与「好备份恢复后数据一致」。
//!
//! 全程只看不写正式库：备份建在临时目录，恢复目标也是临时副本，现有数据不受影响。
//! 用法：
//!   cargo run -p thought-forge-core --example forge_restore_probe -- <forge.db 路径>

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use thought_forge_core::backup;
use thought_forge_core::db;

/// 数一遍各表行数，用来比对「恢复后是否一致」。
fn counts(conn: &Connection) -> Vec<(&'static str, i64)> {
    [
        ("masters", "masters"),
        ("master_units", "master_units"),
        ("council_sessions", "council_sessions"),
        ("council_turns", "council_turns"),
        ("llm_calls", "llm_calls"),
        ("connectors", "connectors"),
        ("backups", "backups"),
    ]
    .iter()
    .map(|(label, table)| {
        let value = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(-1);
        (*label, value)
    })
    .collect()
}

fn main() {
    let Some(db_path) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("用法：forge_restore_probe <forge.db 路径>");
        std::process::exit(2);
    };
    if let Err(error) = run(&db_path) {
        eprintln!("备份探针失败：{error}");
        std::process::exit(1);
    }
}

fn run(db_path: &Path) -> Result<(), String> {
    let work = std::env::temp_dir().join("thought-forge-restore-probe");
    std::fs::create_dir_all(&work).map_err(|error| error.to_string())?;

    let (conn, version) = db::initialize(db_path).map_err(|error| error.to_string())?;
    println!("库：{}（版本 {version}）", db_path.display());
    println!("临时目录：{}", work.display());
    let before = counts(&conn);

    // 按应用同一套调用建一份备份。
    let created = backup::create(&conn, &work, backup::KIND_MANUAL).map_err(|e| e.to_string())?;
    println!(
        "备份：{} 字节，架构版本 {}，校验和 {}",
        created.size_bytes,
        created.schema_version,
        &created.checksum[..created.checksum.len().min(12)]
    );

    // 一、好备份先过校验。
    let verified = backup::verify(Path::new(&created.path)).map_err(|error| error.to_string())?;
    println!("[好备份] 校验通过，{} 字节", verified.size_bytes);

    // 二、坏备份必须被拒：就地改掉中间一个字节，再走应用同一套恢复前校验。
    let tampered_path = PathBuf::from(&created.path);
    let good_copy = work.join("good-copy.sqlite3");
    std::fs::copy(&tampered_path, &good_copy).map_err(|error| error.to_string())?;
    let mut bytes = std::fs::read(&tampered_path).map_err(|error| error.to_string())?;
    let middle = bytes.len() / 2;
    bytes[middle] = bytes[middle].wrapping_add(0x5a);
    std::fs::write(&tampered_path, &bytes).map_err(|error| error.to_string())?;
    match backup::restore_prepare(&conn, &tampered_path) {
        Ok(outcome) => {
            return Err(format!(
                "坏备份竟然通过了恢复前校验（{} 字节），这条防线没生效",
                outcome.size_bytes
            ))
        }
        Err(error) => println!("[坏备份] 依预期被拒：{error}"),
    }

    // 三、把好备份放回原位，走恢复前校验，并把内容与现库逐表比对。
    std::fs::copy(&good_copy, &tampered_path).map_err(|error| error.to_string())?;
    let prepared = backup::restore_prepare(&conn, &tampered_path).map_err(|e| e.to_string())?;
    println!("[好备份] 恢复前校验通过，架构版本 {}", prepared.schema_version);
    let restored_path = work.join("restored.sqlite3");
    std::fs::copy(&tampered_path, &restored_path).map_err(|error| error.to_string())?;
    let restored = Connection::open(&restored_path).map_err(|error| error.to_string())?;
    let after = counts(&restored);

    let mut mismatched: Vec<String> = Vec::new();
    for ((label, left), (_, right)) in before.iter().zip(after.iter()) {
        let mark = if left == right { "一致" } else { "不一致" };
        println!("  {label}：现库 {left} / 恢复后 {right} · {mark}");
        if left != right {
            mismatched.push((*label).to_string());
        }
    }

    let _ = std::fs::remove_dir_all(&work);
    if mismatched.is_empty() {
        println!("结论：坏备份被拒、好备份恢复后逐表一致。");
        Ok(())
    } else {
        Err(format!("恢复后这些表对不上：{}", mismatched.join("、")))
    }
}
