//! 真机迁移探针：在临时库里造一个「旧版本库」，走应用同一套迁移入口，
//! 验「迁移前自动备份」确实先生成、再迁移。
//!
//! 全程在临时目录，不碰正式库。用法：
//!   cargo run -p thought-forge-core --example forge_migration_probe

use std::path::Path;

use rusqlite::Connection;

use thought_forge_core::db::migrations::{apply_all_with_dir, current_version, latest_version, MIGRATIONS};

/// 造一个停在 `target` 版本的库：自己建账本表，按顺序把旧迁移跑一遍。
fn build_old_db(path: &Path, target: i64) -> Result<(), String> {
    let conn = Connection::open(path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
         );",
    )
    .map_err(|error| error.to_string())?;
    for migration in MIGRATIONS.iter().filter(|item| item.version <= target) {
        conn.execute_batch(migration.sql)
            .map_err(|error| format!("跑迁移 {} 失败：{error}", migration.version))?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![migration.version, migration.name],
        )
        .map_err(|error| error.to_string())?;
    }
    let version = current_version(&conn).map_err(|error| error.to_string())?;
    println!("造好的旧库：{}（版本 {version}）", path.display());
    Ok(())
}

fn list_dir(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|items| {
            items
                .flatten()
                .map(|item| item.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn main() {
    let work = std::env::temp_dir().join("thought-forge-migration-probe");
    let _ = std::fs::remove_dir_all(&work);
    if let Err(error) = std::fs::create_dir_all(&work) {
        eprintln!("建临时目录失败：{error}");
        std::process::exit(1);
    }
    let db_path = work.join("old-forge.db");
    let backup_dir = work.join("backups");
    let target = latest_version() - 1;

    if let Err(error) = build_old_db(&db_path, target) {
        eprintln!("{error}");
        std::process::exit(1);
    }

    let before = current_version(&Connection::open(&db_path).unwrap()).unwrap_or(-1);
    let mut conn = Connection::open(&db_path).unwrap();
    match apply_all_with_dir(&mut conn, Some(&backup_dir)) {
        Ok(version) => println!("迁移完成，版本 {before} → {version}"),
        Err(error) => {
            eprintln!("迁移失败：{error}");
            std::process::exit(1);
        }
    }

    let files = list_dir(&backup_dir);
    println!("备份目录 {} 里有 {} 个文件：", backup_dir.display(), files.len());
    for name in &files {
        println!("  {name}");
    }
    let pre = files
        .iter()
        .filter(|name| name.contains("pre_migration") || name.contains("pre-migration"))
        .count();
    let recorded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM backups WHERE kind = 'pre_migration'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1);
    println!("迁移前备份：文件 {pre} 个，账本 {recorded} 条");

    let _ = std::fs::remove_dir_all(&work);
    if pre == 0 {
        eprintln!("结论：迁移前没有生成自动备份，这条防线没生效");
        std::process::exit(1);
    }
    println!("结论：旧库升级时先生成了迁移前自动备份，再完成迁移。");
}
