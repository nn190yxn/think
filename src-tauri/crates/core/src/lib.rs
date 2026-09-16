//! 思想熔炉的纯逻辑内核。
//!
//! 这一层不依赖任何界面或运行时外壳，因此可以在没有系统 WebView 库的
//! 环境下直接测试。桌面外壳只负责把这里的返回值包装成 command 边界。

pub mod asset;
pub mod backup;
pub mod capture;
pub mod companion;
pub mod connector;
pub mod cost;
pub mod council;
pub mod corpus;
pub mod credential;
pub mod data;
pub mod db;
pub mod distill;
pub mod error;
pub mod furnace;
pub mod kb;
pub mod llm;
pub mod master;
pub mod network;
/// 自我蒸馏。模块名避开 Rust 关键字 `self`，目录仍按领域命名为 `self/`。
#[path = "self/mod.rs"]
pub mod self_distill;
pub mod util;

pub use error::{CoreError, CoreResult};

/// 应用身份，供 `app_info` 命令使用。
pub const APP_NAME: &str = "思想熔炉";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub schema_version: i64,
}

pub fn app_info(conn: &rusqlite::Connection) -> CoreResult<AppInfo> {
    Ok(AppInfo {
        name: APP_NAME,
        version: APP_VERSION,
        schema_version: db::migrations::current_version(conn)?,
    })
}
