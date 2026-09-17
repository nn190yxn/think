//! 运行控制：取消请求、心跳与中断会话识别。
//!
//! 取消只在轮次边界生效，已完成轮次全部保留；心跳由编排在每次席位调用后刷新，
//! 启动时按心跳间隔判定哪些运行中会话已经中断。

use rusqlite::Connection;

use crate::error::CoreResult;

use super::{repo, DivergenceView, SessionView};

/// 被取消的会话状态。
pub const STATUS_CANCELLED: &str = "cancelled";
/// 运行中的会话状态。
pub const STATUS_RUNNING: &str = "running";

/// 请求取消：已结束的会话按幂等处理，不修改已完成轮次。
pub fn request_cancel(conn: &Connection, session_id: &str) -> CoreResult<()> {
    repo::request_cancel(conn, session_id)
}

/// 刷新心跳时刻。
pub fn heartbeat(conn: &Connection, session_id: &str) -> CoreResult<()> {
    repo::heartbeat(conn, session_id)
}

/// 中断会话：状态为运行中且心跳早于阈值的会话。
pub fn recoverable(conn: &Connection, stale_seconds: i64) -> CoreResult<Vec<SessionView>> {
    repo::recoverable(conn, stale_seconds)
}

/// 本轮边界是否需要停止追加。
pub fn cancel_requested(conn: &Connection, session_id: &str) -> CoreResult<bool> {
    repo::cancel_requested(conn, session_id)
}

/// 取消生效：置状态与取消时刻，保留结论与已完成轮次。
pub fn mark_cancelled(
    conn: &Connection,
    session_id: &str,
    conclusion: &str,
    divergences: &[DivergenceView],
) -> CoreResult<()> {
    repo::mark_cancelled(conn, session_id, conclusion, divergences)
}
