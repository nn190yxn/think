use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use thought_forge_core::db;
use thought_forge_core::CoreResult;

use crate::capture::ShellCapture;

pub struct AppState {
    pub conn: Mutex<Connection>,
    pub db_path: PathBuf,
    /// 常驻采集源：文件监听跨多次采集持续积累事件。
    pub capture: ShellCapture,
}

impl AppState {
    pub fn new(conn: Connection, db_path: PathBuf, capture: ShellCapture) -> Self {
        Self {
            conn: Mutex::new(conn),
            db_path,
            capture,
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
    // 关注目录来自设置；路径无效时在解析阶段就被剔除。
    let roots = ShellCapture::roots_from_setting(
        db::settings::get(&conn, crate::capture::WATCH_ROOTS_KEY)?.as_deref(),
    );
    // 监听建立失败时退化为无文件采集，不让采集侧的问题挡住应用启动。
    let capture = ShellCapture::new(roots).or_else(|_| ShellCapture::new(Vec::new()))?;
    Ok(AppState::new(conn, db_path, capture))
}
