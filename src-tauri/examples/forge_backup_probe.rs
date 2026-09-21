//! 备份探针：按界面「备份与恢复」的同一套调用建一份备份，供真机验收核对 V16。
//!
//! 用法：forge_backup_probe <forge.db 路径> [备份目录]
//! 备份目录默认取数据库同级的 `backups`，与界面里点「立即备份」落的地方一致。

use std::path::{Path, PathBuf};

use thought_forge_core::backup;
use thought_forge_core::db;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(db_path) = args.next() else {
        eprintln!("用法：forge_backup_probe <forge.db 路径> [备份目录]");
        std::process::exit(2);
    };
    let db_path = PathBuf::from(db_path);
    let dir = args.next().map(PathBuf::from).unwrap_or_else(|| {
        db_path
            .parent()
            .map(|parent| parent.join("backups"))
            .unwrap_or_else(|| PathBuf::from("backups"))
    });

    if let Err(error) = run(&db_path, &dir) {
        eprintln!("备份失败：{error}");
        std::process::exit(1);
    }
}

fn run(db_path: &Path, dir: &Path) -> Result<(), String> {
    let (conn, version) = db::initialize(db_path).map_err(|error| error.to_string())?;
    println!("库：{}（版本 {version}）", db_path.display());
    println!("备份目录：{}", dir.display());

    let outcome =
        backup::create(&conn, dir, backup::KIND_MANUAL).map_err(|error| error.to_string())?;
    println!("备份文件：{}", outcome.path);
    println!(
        "大小 {} 字节 · 架构版本 {} · 类型 {} · 生成于 {}",
        outcome.size_bytes, outcome.schema_version, outcome.kind, outcome.created_at
    );
    println!("摘要：{}", outcome.checksum);

    // 复核一遍：文件在位，且能被独立校验。
    let verified =
        backup::verify(&PathBuf::from(&outcome.path)).map_err(|error| error.to_string())?;
    println!("复核：{}（{} 字节）", verified.path, verified.size_bytes);

    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM backups", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    println!("备份表记录：{rows} 条");
    Ok(())
}
