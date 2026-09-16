use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use thought_forge_core::db;
use thought_forge_core::CoreResult;

pub struct AppState {
    pub conn: Mutex<Connection>,
    pub db_path: PathBuf,
}

impl AppState {
    pub fn new(conn: Connection, db_path: PathBuf) -> Self {
        Self {
            conn: Mutex::new(conn),
            db_path,
        }
    }
}

/// 数据库放在应用数据目录，随用户配置漫游而不落在安装目录。
pub fn resolve_db_path(app: &AppHandle) -> CoreResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| thought_forge_core::CoreError::InvalidInput(error.to_string()))?;
    Ok(dir.join("forge.db"))
}

pub fn initialize_state(app: &AppHandle) -> CoreResult<AppState> {
    let db_path = resolve_db_path(app)?;
    let (conn, _version) = db::initialize(&db_path)?;
    Ok(AppState::new(conn, db_path))
}
