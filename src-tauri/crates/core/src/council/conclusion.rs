//! 会诊结论详情页的数据组装：结论、分歧、收敛过程、逐席依据、外部来源、演化链。
//!
//! 逐轮内容已落在 `council_turns`，分歧摘要落在 `council_sessions.divergences_json`，
//! 检索快照落在 `council_sources`，本模块只做组织与呈现，不新增表。

use rusqlite::Connection;

use crate::connector::service as connector_service;
use crate::cost;
use crate::error::CoreResult;
use crate::llm::platform;
use crate::network::repo as network_repo;

use super::{repo, speech, ConclusionView, SessionView};

/// 组装结论详情页。
pub fn conclusion_view(conn: &Connection, session_id: &str) -> CoreResult<ConclusionView> {
    let session = repo::get_session(conn, session_id)?;
    let rotation = repo::latest_rotation(conn, session_id)?.unwrap_or(0);
    let turns = repo::turns(conn, session_id, None)?;
    let prompt_version = turns
        .iter()
        .rev()
        .find(|turn| !turn.prompt_version.trim().is_empty())
        .map(|turn| turn.prompt_version.clone())
        .unwrap_or_else(|| super::orchestrator::PROMPT_VERSION.to_string());
    let sources = if connector_table_exists(conn)? {
        connector_service::sources(conn, session_id, rotation)?
    } else {
        Vec::new()
    };
    // 调用次数取实际落库记录，费用按这些次数与当前单价估算。
    let llm_calls = turns.len() as i64;
    let search_calls = sources.len() as i64;
    let estimate = cost::estimate_calls(conn, llm_calls, search_calls)?;
    let currency = platform::enabled(conn)?
        .map(|item| item.currency)
        .unwrap_or_else(|| cost::DEFAULT_CURRENCY.to_string());
    Ok(ConclusionView {
        metrics: repo::metrics(conn, session_id, rotation)?,
        speeches: speech::seat_speech(conn, session_id, rotation)?,
        sources,
        history: history(conn, &session)?,
        prompt_version,
        llm_calls,
        search_calls,
        cost_micros: estimate.cost_micros,
        currency,
        priced: estimate.priced,
        session,
    })
}

/// 同一主题键下的其它会诊，按时间倒序，用于演化链。
fn history(conn: &Connection, session: &SessionView) -> CoreResult<Vec<SessionView>> {
    let topic_key = network_repo::normalize(&session.question);
    if topic_key.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT DISTINCT session_id FROM thought_records
         WHERE topic_key = ?1 AND session_id IS NOT NULL AND session_id != ?2
         ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![topic_key, session.id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut history = Vec::new();
    for row in rows {
        let id = row?;
        if let Ok(view) = repo::get_session(conn, &id) {
            history.push(view);
        }
    }
    Ok(history)
}

/// 迁移尚未执行到连接器版本时，界面按「未启用外部检索」呈现空态。
fn connector_table_exists(conn: &Connection) -> CoreResult<bool> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'council_sources'",
        [],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}
