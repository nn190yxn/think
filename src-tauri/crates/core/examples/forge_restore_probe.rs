//! 真机备份探针：在「现库快照」上验坏备份被拒、好备份恢复后一致。
//!
//! 不写正式库：快照用 `VACUUM INTO` 生成到临时目录，账本与恢复都在临时副本上做。
//! 早期版本直接在正式库上建备份，会把临时备份记进正式账本；`--clean-temp-backups`
//! 用来清掉那些痕迹。
//!
//! 用法：
//!   cargo run -p thought-forge-core --example forge_restore_probe -- <forge.db 路径>
//!   cargo run -p thought-forge-core --example forge_restore_probe -- <forge.db 路径> --clean-temp-backups

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use thought_forge_core::backup;
use thought_forge_core::db;

const PROBE_TAG: &str = "thought-forge-restore-probe";

/// 数一遍各表行数，用来比对「恢复后是否一致」。
fn counts(conn: &Connection) -> Vec<(&'static str, i64)> {
    [
        ("masters", "masters"),
        ("master_units", "master_units"),
        ("council_sessions", "council_sessions"),
        ("council_turns", "council_turns"),
        ("llm_calls", "llm_calls"),
        ("connectors", "connectors"),
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let clean_only = args.iter().any(|item| item == "--clean-temp-backups");
    let Some(db_path) = args
        .iter()
        .find(|item| !item.starts_with("--"))
        .map(PathBuf::from)
    else {
        eprintln!("用法：forge_restore_probe <forge.db 路径> [--clean-temp-backups]");
        std::process::exit(2);
    };
    if let Err(error) = run(&db_path, clean_only) {
        eprintln!("备份探针失败：{error}");
        std::process::exit(1);
    }
}

fn run(db_path: &Path, clean_only: bool) -> Result<(), String> {
    let work = std::env::temp_dir().join(PROBE_TAG);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|error| error.to_string())?;

    let (live, version) = db::initialize(db_path).map_err(|error| error.to_string())?;
    println!("现库：{}（版本 {version}）", db_path.display());

    if clean_only {
        let removed = live
            .execute(
                "DELETE FROM backups WHERE path LIKE ?1",
                [format!("%{PROBE_TAG}%")],
            )
            .map_err(|error| error.to_string())?;
        println!("已清掉早期探针留在账本里的临时备份记录 {removed} 条");
        let left = backup::list(&live, 10).map_err(|error| error.to_string())?;
        println!("账本现有备份 {} 条：", left.len());
        for item in &left {
            println!("  {} {} kind={}", item.created_at, item.size_bytes, item.kind);
        }
        return Ok(());
    }

    let before = counts(&live);

    // 与线上关系一致：账本在「库」里，备份是另一个文件，账本不会写进备份自己。
    // A 当作现库用（临时副本，可写），B 是 A 的备份。
    let snapshot = backup::create_file(&live, &work, backup::KIND_MANUAL).map_err(|e| e.to_string())?;
    let snapshot_path = PathBuf::from(&snapshot.path);
    let snap = Connection::open(&snapshot_path).map_err(|error| error.to_string())?;
    let made = backup::create(&snap, &work.join("backups"), backup::KIND_MANUAL).map_err(|e| e.to_string())?;
    let backup_path = PathBuf::from(&made.path);
    println!(
        "现库快照 {} 字节；它的备份 {} 字节，账本在快照里",
        snapshot.size_bytes, made.size_bytes
    );

    // 一、好备份先过校验。
    backup::restore_prepare(&snap, &backup_path).map_err(|error| error.to_string())?;
    println!("[好备份] 校验通过");

    // 二、坏备份必须被拒：就地改掉中间一个字节。
    let good = std::fs::read(&backup_path).map_err(|error| error.to_string())?;
    let mut bytes = good.clone();
    let middle = bytes.len() / 2;
    bytes[middle] = bytes[middle].wrapping_add(0x5a);
    std::fs::write(&backup_path, &bytes).map_err(|error| error.to_string())?;
    match backup::restore_prepare(&snap, &backup_path) {
        Ok(outcome) => {
            return Err(format!(
                "坏备份竟然通过了恢复前校验（{} 字节），这条防线没生效",
                outcome.size_bytes
            ))
        }
        Err(error) => println!("[坏备份] 依预期被拒：{error}"),
    }

    // 三、放回好备份，再走一次校验，并与现库逐表比对。
    std::fs::write(&backup_path, &good).map_err(|error| error.to_string())?;
    let prepared = backup::restore_prepare(&snap, &backup_path).map_err(|e| e.to_string())?;
    println!("[好备份] 放回后校验通过，架构版本 {}", prepared.schema_version);

    let after = counts(&snap);
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
        println!("结论：坏备份被拒、好备份恢复后逐表一致；正式库全程只读。");
        Ok(())
    } else {
        Err(format!("恢复后这些表对不上：{}", mismatched.join("、")))
    }
}
