//! 会诊结论详情页的数据组装：结论、分歧、收敛过程、逐席依据、外部来源、演化链。
//!
//! 逐轮内容已落在 `council_turns`，分歧摘要落在 `council_sessions.divergences_json`，
//! 检索快照落在 `council_sources`，本模块只做组织与呈现，不新增表。

use rusqlite::Connection;

use crate::connector::service as connector_service;
use crate::cost;
use crate::error::CoreResult;
use crate::llm::platform;
use crate::master::LAYER_ORDER;
use crate::network::repo as network_repo;

use super::{repo, scoring, speech, ConclusionView, SessionView, StanceChange};

/// 立场变化的两条判定线：重合度不低于前者算延续，低于后者算转向，中间算调整。
const STANCE_SAME_THRESHOLD: f64 = 0.6;
const STANCE_SHIFT_THRESHOLD: f64 = 0.25;

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
        stance_changes: stance_changes(conn, &session, rotation)?,
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

/// 每题立场与上一次同主题会诊相比的变化。
///
/// 没有本场立场记录（升级前的会话）或找不到上一次同主题会诊时返回空表，
/// 界面据此隐藏这一段。
fn stance_changes(
    conn: &Connection,
    session: &SessionView,
    rotation: i64,
) -> CoreResult<Vec<StanceChange>> {
    let current = repo::stances(conn, &session.id, rotation)?;
    if current.is_empty() {
        return Ok(Vec::new());
    }
    let Some(previous_session_id) = previous_same_topic(conn, session)? else {
        return Ok(Vec::new());
    };
    let previous_rotation = repo::latest_rotation(conn, &previous_session_id)?.unwrap_or(0);
    let previous = repo::stances(conn, &previous_session_id, previous_rotation)?;
    if previous.is_empty() {
        return Ok(Vec::new());
    }

    let mut changes = Vec::new();
    for layer in LAYER_ORDER {
        let now = current.iter().find(|stance| stance.layer == layer);
        let before = previous.iter().find(|stance| stance.layer == layer);
        match (now, before) {
            (Some(now), Some(before)) => {
                let similarity = scoring::overlap(
                    &scoring::tokens(&now.summary),
                    &scoring::tokens(&before.summary),
                );
                changes.push(StanceChange {
                    layer,
                    master_id: now.master_id.clone(),
                    master_name: now.master_name.clone(),
                    previous_master_name: Some(before.master_name.clone()),
                    summary: now.summary.clone(),
                    previous_summary: Some(before.summary.clone()),
                    similarity,
                    change: stance_change_code(similarity).to_string(),
                });
            }
            (Some(now), None) => changes.push(StanceChange {
                layer,
                master_id: now.master_id.clone(),
                master_name: now.master_name.clone(),
                previous_master_name: None,
                summary: now.summary.clone(),
                previous_summary: None,
                similarity: 0.0,
                change: "new".to_string(),
            }),
            (None, Some(before)) => changes.push(StanceChange {
                layer,
                master_id: before.master_id.clone(),
                master_name: before.master_name.clone(),
                previous_master_name: Some(before.master_name.clone()),
                summary: String::new(),
                previous_summary: Some(before.summary.clone()),
                similarity: 0.0,
                change: "dropped".to_string(),
            }),
            (None, None) => {}
        }
    }
    Ok(changes)
}

fn stance_change_code(similarity: f64) -> &'static str {
    if similarity >= STANCE_SAME_THRESHOLD {
        "same"
    } else if similarity >= STANCE_SHIFT_THRESHOLD {
        "adjusted"
    } else {
        "shifted"
    }
}

/// 上一次同主题会诊：按写入顺序往前找，取题面归一化后一致的第一场。
fn previous_same_topic(conn: &Connection, session: &SessionView) -> CoreResult<Option<String>> {
    let topic = network_repo::normalize(&session.question);
    if topic.is_empty() {
        return Ok(None);
    }
    let mut stmt = conn.prepare(
        "SELECT id, question FROM council_sessions
         WHERE id != ?1 AND rowid < (SELECT rowid FROM council_sessions WHERE id = ?1)
         ORDER BY rowid DESC LIMIT 50",
    )?;
    let rows = stmt.query_map([&session.id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, question) = row?;
        if network_repo::normalize(&question) == topic {
            return Ok(Some(id));
        }
    }
    Ok(None)
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
