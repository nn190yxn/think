//! 逐席发言视图与失败席位单轮重试。
//!
//! 发言记录复用 `council_turns`，不新增表。同一席位同一轮出现多条记录时
//! （例如失败后重试），以最后写入的一条为有效记录。

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::error::{CoreError, CoreResult};
use crate::llm::{call_model, ModelClient, RetryPolicy};
use crate::master::{repo as master_repo, Layer};

use super::orchestrator::{cross_prompt, independent_prompt};
use super::{repo, SeatSpeech, RoundSpeech};

/// 汇总某个阵容轮次下每个席位的发言与状态。
pub fn seat_speech(
    conn: &Connection,
    session_id: &str,
    rotation: i64,
) -> CoreResult<Vec<SeatSpeech>> {
    let panel = repo::panels(conn, session_id)?
        .into_iter()
        .find(|panel| panel.rotation == rotation)
        .ok_or_else(|| CoreError::NotFound(format!("会诊 {session_id} 的第 {rotation} 次阵容")))?;
    let turns = repo::turns(conn, session_id, Some(rotation))?;

    let mut seats = Vec::new();
    for master_id in &panel.master_ids {
        let master = master_repo::detail(conn, master_id).ok();
        let master_name = master
            .as_ref()
            .map(|master| master.name.clone())
            .unwrap_or_else(|| master_id.clone());
        // 以阵容记录的席位指派为准；历史阵容缺映射时按大师层次回退。
        let layer = panel
            .layer_of(master_id)
            .or_else(|| master.as_ref().map(|master| super::primary_layer(&master.layers)))
            .unwrap_or(Layer::Fa);

        // 按轮次收集，后写入的记录覆盖先写入的，因此重试结果自然生效。
        let mut by_round: BTreeMap<i64, RoundSpeech> = BTreeMap::new();
        for turn in turns.iter().filter(|turn| {
            turn.master_id.as_deref() == Some(master_id.as_str())
                && matches!(turn.role.as_str(), "answer" | "cross")
        }) {
            by_round.insert(
                turn.round,
                RoundSpeech {
                    round: turn.round,
                    role: turn.role.clone(),
                    content: turn.content.clone(),
                    status: turn.status.clone(),
                    error_code: turn.error_code.clone(),
                    created_at: turn.created_at.clone(),
                },
            );
        }

        let rounds: Vec<RoundSpeech> = by_round.into_values().collect();
        let status = if rounds.is_empty() {
            "pending"
        } else if rounds.iter().any(|round| round.status != "ok") {
            "failed"
        } else {
            "answered"
        };

        seats.push(SeatSpeech {
            master_id: master_id.clone(),
            master_name,
            layer,
            status: status.to_string(),
            rounds,
        });
    }
    Ok(seats)
}

/// 重试某个席位在某一轮的发言，只重跑该席位该轮，不重跑整场。
///
/// 成功或失败都追加一条新的轮次记录，既有记录保持可读。返回重试后的席位视图。
pub fn retry_seat(
    conn: &Connection,
    client: &dyn ModelClient,
    session_id: &str,
    rotation: i64,
    master_id: &str,
    round: i64,
    policy: &RetryPolicy,
) -> CoreResult<Vec<SeatSpeech>> {
    let session = repo::get_session(conn, session_id)?;
    let panel = repo::panels(conn, session_id)?
        .into_iter()
        .find(|panel| panel.rotation == rotation)
        .ok_or_else(|| CoreError::NotFound(format!("会诊 {session_id} 的第 {rotation} 次阵容")))?;
    let turns = repo::turns(conn, session_id, Some(rotation))?;

    let role = turns
        .iter()
        .find(|turn| turn.master_id.as_deref() == Some(master_id) && turn.round == round)
        .map(|turn| turn.role.clone())
        .ok_or_else(|| {
            CoreError::InvalidInput(format!("第 {round} 轮没有该席位的发言记录，无法重试"))
        })?;

    let master = master_repo::detail(conn, master_id)?;
    let request = if role == "answer" {
        let layer = panel
            .layer_of(master_id)
            .unwrap_or_else(|| super::primary_layer(&master.layers));
        independent_prompt(&session.question, &master, layer)
    } else {
        let history = history_before(conn, &turns, round)?;
        cross_prompt(&session.question, &master, &history)
    };

    let (status, content, error_code) = match call_model(conn, client, &request, policy) {
        Ok(response) => ("ok".to_string(), response.content, None),
        Err(error) => ("failed".to_string(), String::new(), Some(error.code().to_string())),
    };
    repo::save_turn(
        conn,
        &repo::NewTurn {
            session_id: session_id.to_string(),
            round,
            panel_rotation: panel.rotation,
            role,
            master_id: Some(master_id.to_string()),
            master_version: Some(master.current_version),
            content,
            citations: Vec::new(),
            prompt_version: super::orchestrator::PROMPT_VERSION.to_string(),
            status,
            error_code,
        },
    )?;

    seat_speech(conn, session_id, rotation)
}

/// 取目标轮次之前的成功发言，按轮次与写入顺序拼接，供交叉质询使用。
fn history_before(
    conn: &Connection,
    turns: &[super::TurnView],
    round: i64,
) -> CoreResult<Vec<Vec<(String, String)>>> {
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut history: Vec<Vec<(String, String)>> = Vec::new();
    let mut current_round = 0i64;
    for turn in turns.iter().filter(|turn| {
        turn.round < round && turn.status == "ok" && matches!(turn.role.as_str(), "answer" | "cross")
    }) {
        if turn.round != current_round {
            history.push(Vec::new());
            current_round = turn.round;
        }
        let Some(master_id) = turn.master_id.as_deref() else {
            continue;
        };
        let name = match names.get(master_id) {
            Some(name) => name.clone(),
            None => {
                let name = master_repo::detail(conn, master_id)
                    .map(|master| master.name)
                    .unwrap_or_else(|_| master_id.to_string());
                names.insert(master_id.to_string(), name.clone());
                name
            }
        };
        if let Some(block) = history.last_mut() {
            block.push((name, turn.content.clone()));
        }
    }
    Ok(history)
}
