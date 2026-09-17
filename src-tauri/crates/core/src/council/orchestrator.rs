//! 会诊编排：解析、选角、第一轮隔离作答、第二轮交叉质询与收敛裁决。
//!
//! 第一轮的提示词只包含问题与该大师自己的技能单元，不包含同轮任何其他
//! 大师的输出，这是保证答案独立性的硬性约束。原始语料正文不进入提示词，
//! 只使用大师包中已蒸馏的技能单元。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::connector::service::{self as connector_service, Retrieval};
use crate::cost;
use crate::error::{CoreError, CoreResult};
use crate::llm::{call_model, ModelClient, ModelRequest, RetryPolicy};
use crate::master::{repo as master_repo, Layer, MasterDetail, MasterUnitView};

use super::{control, repo, CouncilOutcome, DivergenceView, PanelView, SessionView};

const ROUND_ANSWER: i64 = 1;
const FIRST_CROSS_ROUND: i64 = 2;
/// 交叉质询累计历史的总长度上限，超出时只保留最近轮次。
pub const MAX_CROSS_CONTEXT_CHARS: usize = 6000;
/// 会诊提示词模板版本。每次改动会诊提示词时必须递增，与大师包版本共同构成复现条件。
pub const PROMPT_VERSION: &str = "2026-09-17.1";

fn unit_block(unit: &MasterUnitView) -> String {
    let steps = unit
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {step}", index + 1))
        .collect::<Vec<_>>()
        .join("；");
    format!(
        "【技能单元】{title}（{layer}层）\n触发条件：{trigger}\n执行步骤：{steps}\n作用机制：{mechanism}\n适用边界：{boundary}",
        title = unit.title,
        layer = unit.layer.name(),
        trigger = unit.trigger_condition,
        steps = steps,
        mechanism = unit.mechanism,
        boundary = unit.boundary,
    )
}

/// 第一轮提示词：只含问题、该席位被指派到的题与该大师自己的技能单元。
///
/// 六题会诊里每个席位只负责一问，因此提示词明确指定题与核心问题，
/// 允许席位回答「这一题我的积累不够」，避免把不相干的框架硬套上去。
pub fn independent_prompt(question: &str, master: &MasterDetail, layer: Layer) -> ModelRequest {
    let units = master
        .units
        .iter()
        .map(unit_block)
        .collect::<Vec<_>>()
        .join("\n\n");
    ModelRequest::new(
        "council_round1",
        format!(
            "你是「{}」，领域是「{}」。请只使用你自己的技能单元回答，\
             不要引用、猜测或提及任何其他大师的观点。若你的技能单元不适用于该问题，\
             说明不适用的原因。回答控制在 400 字以内，并标注你使用了哪条技能单元。\
             本轮你负责回答的是「{question}」这一问，请从这一问的角度给出你的判断；\
             若你在这方面的积累不足，直接说明，不要套用不相干的框架。",
            master.name,
            master.domain,
            question = layer.question(),
        ),
        format!(
            "问题：{question}\n\n本轮你负责的题：{}（{}）\n\n你的技能单元：\n{units}",
            layer.name(),
            layer.question(),
        ),
    )
    .with_prompt_version(PROMPT_VERSION)
}

/// 带外部资料的独立作答提示词。共享背景注入全席，席位补充只进本席。
pub fn independent_prompt_with_sources(
    question: &str,
    master: &MasterDetail,
    sources: &str,
    layer: Layer,
) -> ModelRequest {
    let mut request = independent_prompt(question, master, layer);
    if !sources.trim().is_empty() {
        request.user = format!("{sources}\n\n{}", request.user);
    }
    request
}

/// 把逐轮发言渲染为累计历史，超出上限时保留最近轮次。
pub fn render_history(history: &[Vec<(String, String)>]) -> String {
    let blocks: Vec<String> = history
        .iter()
        .enumerate()
        .map(|(index, round)| {
            let mut block = format!("【第 {} 轮】", index + 1);
            for (name, content) in round {
                block.push_str(&format!("\n【{name}】{content}"));
            }
            block
        })
        .collect();

    let mut start = blocks.len();
    let mut total = 0usize;
    while start > 0 {
        let length = blocks[start - 1].chars().count();
        if total + length > MAX_CROSS_CONTEXT_CHARS {
            break;
        }
        total += length;
        start -= 1;
    }
    if start == blocks.len() && !blocks.is_empty() {
        // 最近的单块就超限：截断该块，保证仍然有上下文可用。
        return blocks[blocks.len() - 1]
            .chars()
            .take(MAX_CROSS_CONTEXT_CHARS)
            .collect();
    }
    blocks[start..].join("\n\n")
}

/// 交叉质询提示词：带上此前全部轮次的往复，彼此质询。
pub fn cross_prompt(
    question: &str,
    master: &MasterDetail,
    history: &[Vec<(String, String)>],
) -> ModelRequest {
    let joined = render_history(history);
    ModelRequest::new(
        "council_cross",
        format!(
            "你是「{}」。请审阅同席其他大师的判断，逐条指出你认为的错误、盲区与\
             未言明的前提假设，并说明你是否因此调整自己的判断。不要复述对方原话。",
            master.name
        ),
        format!("问题：{question}\n\n此前各轮发言：\n{joined}"),
    )
    .with_prompt_version(PROMPT_VERSION)
}

/// 带外部资料的交叉质询提示词。
pub fn cross_prompt_with_sources(
    question: &str,
    master: &MasterDetail,
    history: &[Vec<(String, String)>],
    sources: &str,
) -> ModelRequest {
    let mut request = cross_prompt(question, master, history);
    if !sources.trim().is_empty() {
        request.user = format!("{sources}\n\n{}", request.user);
    }
    request
}

/// 收敛提示词：综合累计历史生成结论。
pub fn synthesis_prompt(question: &str, history: &[Vec<(String, String)>]) -> ModelRequest {
    let joined = render_history(history);
    ModelRequest::new(
        "council_synthesis",
        "你是这场会诊的编排方。请综合各位大师的判断与质询，给出一个可执行的结论，\
         说明在什么条件下该结论成立，并明确指出仍未解决的争议。",
        format!("问题：{question}\n\n各轮发言：\n{joined}"),
    )
    .with_prompt_version(PROMPT_VERSION)
}

/// 一位席位在本轮的作答，含它被指派到的题。
pub struct SeatAnswer {
    pub name: String,
    pub content: String,
    pub layer: Layer,
}

/// 由答案文本推导分歧点，每条都标明落在哪一题。
///
/// 两处来源：同一题上有两位以上发言者时的对立；以及某位席位的判断
/// 与全场其余席位差异最大时，按它自己那一题归档的突出分歧。
pub fn derive_divergences(answers: &[SeatAnswer]) -> Vec<DivergenceView> {
    if answers.len() < 2 {
        return Vec::new();
    }
    let token_sets: Vec<BTreeSet<String>> = answers
        .iter()
        .map(|answer| super::scoring::tokens(&answer.content))
        .collect();

    let mut divergences = Vec::new();
    for layer in crate::master::LAYER_ORDER {
        let indexes: Vec<usize> = (0..answers.len())
            .filter(|index| answers[*index].layer == layer)
            .collect();
        if indexes.len() < 2 {
            continue;
        }

        let mut worst_pair: Option<(usize, usize, f64)> = None;
        for (offset, left) in indexes.iter().enumerate() {
            for right in indexes.iter().skip(offset + 1) {
                let similarity = super::scoring::overlap(&token_sets[*left], &token_sets[*right]);
                match worst_pair {
                    Some((_, _, current)) if current <= similarity => {}
                    _ => worst_pair = Some((*left, *right, similarity)),
                }
            }
        }
        if let Some((left, right, similarity)) = worst_pair {
            divergences.push(DivergenceView {
                layer,
                text: format!(
                    "在同一题上，「{}」与「{}」的判断差异最大（用词重合 {:.0}%）",
                    answers[left].name,
                    answers[right].name,
                    similarity * 100.0
                ),
            });
        }
    }

    // 与全场其余席位平均重合度最低的一位：按它所在的题归档，
    // 说明是哪一题的视角与全场最不一样。
    {
        let mut outlier: Option<(usize, f64)> = None;
        for index in 0..answers.len() {
            let total: f64 = (0..answers.len())
                .filter(|other| *other != index)
                .map(|other| super::scoring::overlap(&token_sets[index], &token_sets[other]))
                .sum();
            let average = total / (answers.len() - 1) as f64;
            match outlier {
                Some((_, current)) if current <= average => {}
                _ => outlier = Some((index, average)),
            }
        }
        if let Some((index, average)) = outlier {
            let layer = answers[index].layer;
            divergences.push(DivergenceView {
                layer,
                text: format!(
                    "在「{} · {}」这一题上，「{}」的判断与全场其余席位差异最大\
                     （用词重合 {:.0}%），可能是少数意见或未被理解的盲区",
                    layer.name(),
                    layer.question(),
                    answers[index].name,
                    average * 100.0
                ),
            });
        }
    }

    divergences
}

/// 执行一次会诊：隔离作答、按分歧自适应追加质询轮、收敛裁决，全程写入会话记录。
pub fn run_council(
    conn: &Connection,
    client: &dyn ModelClient,
    session_id: &str,
    policy: &RetryPolicy,
) -> CoreResult<CouncilOutcome> {
    run_council_with_retrieval(conn, client, &Retrieval::none(), session_id, policy)
}

/// 带外部检索的执行入口。检索能力未接入时退化为与 [`run_council`] 相同的行为。
pub fn run_council_with_retrieval(
    conn: &Connection,
    client: &dyn ModelClient,
    retrieval: &Retrieval<'_>,
    session_id: &str,
    policy: &RetryPolicy,
) -> CoreResult<CouncilOutcome> {
    run_council_with_judge(conn, client, retrieval, session_id, policy, None)
}

/// 带外部检索与极性判定的执行入口。判定能力未接入时按词面判定并留下回退标记。
pub fn run_council_with_judge(
    conn: &Connection,
    client: &dyn ModelClient,
    retrieval: &Retrieval<'_>,
    session_id: &str,
    policy: &RetryPolicy,
    judge: Option<&dyn super::divergence::PolarityJudge>,
) -> CoreResult<CouncilOutcome> {
    let session: SessionView = repo::get_session(conn, session_id)?;
    let mut panel: PanelView = repo::latest_panel(conn, session_id)?
        .ok_or_else(|| CoreError::InvalidInput("会诊尚未选角，无法开始".to_string()))?;
    let tuning = super::tuning::snapshot(conn)?;
    let divergence_mode = super::divergence::DivergenceMode::parse(&tuning.divergence_mode)
        .unwrap_or(super::divergence::DivergenceMode::Hybrid);

    // 配额闸门在发起前生效：超限按策略拒绝，降级时把压缩范围写回会话并只跑压缩后的规模。
    let seats = panel.master_ids.len() as i64;
    let searches = if tuning.seat_search { seats + 1 } else { 1 };
    let estimate = cost::estimate(conn, seats, tuning.max_rounds, searches)?;
    let decision = cost::guard(conn, &estimate)?;
    if !decision.allowed {
        return Err(CoreError::InvalidInput(decision.reason));
    }
    if decision.max_rounds.is_some() || decision.max_seats.is_some() {
        repo::set_quota(
            conn,
            session_id,
            &decision.policy,
            decision.max_rounds,
            decision.max_seats,
            &decision.reason,
        )?;
    }
    // 断点续跑时沿用上次压缩范围，避免恢复后超出原定规模。
    let max_rounds = session
        .quota_max_rounds
        .or(decision.max_rounds)
        .unwrap_or(tuning.max_rounds);
    if let Some(max_seats) = decision.max_seats {
        panel.master_ids.truncate(max_seats.max(0) as usize);
    }

    repo::update_status(conn, session_id, "running")?;
    control::heartbeat(conn, session_id)?;
    let mut cancelled = false;

    // 断点续跑：已成功的发言直接复用，不重跑已完成轮次。
    let existing = repo::turns(conn, session_id, Some(panel.rotation))?;
    let ok_turns: BTreeMap<(i64, String), String> = existing
        .iter()
        .filter(|turn| turn.status == "ok")
        .filter_map(|turn| {
            turn.master_id
                .clone()
                .map(|master_id| ((turn.round, master_id), turn.content.clone()))
        })
        .collect();

    // 共享背景在会诊启动时冻结，全席看到的是同一份材料。
    let background = connector_service::collect_background(
        conn,
        retrieval,
        session_id,
        panel.rotation,
        &session.question,
    )?;
    let fetched_at = background
        .first()
        .map(|source| source.fetched_at.clone())
        .unwrap_or_else(|| repo::now(conn).unwrap_or_default());
    let background_block = connector_service::sources_block("共享背景", &background, &fetched_at);

    // 第一轮：每位大师在只有自己技能单元的上下文里独立作答。
    let mut answers: Vec<(String, String)> = Vec::new();
    let mut loaded: Vec<MasterDetail> = Vec::new();
    let mut seat_blocks: Vec<String> = Vec::new();
    let mut failed = 0usize;

    for master_id in &panel.master_ids {
        let master = match master_repo::detail(conn, master_id) {
            Ok(master) => master,
            Err(error) => {
                failed += 1;
                save_failed(conn, session_id, &panel, ROUND_ANSWER, "answer", Some(master_id.clone()), None, &error)?;
                continue;
            }
        };
        // 取消只在席位边界生效，已完成发言全部保留。
        if control::cancel_requested(conn, session_id)? {
            cancelled = true;
            break;
        }
        if let Some(content) = ok_turns.get(&(ROUND_ANSWER, master_id.clone())) {
            answers.push((master.name.clone(), content.clone()));
            loaded.push(master);
            seat_blocks.push(String::new());
            continue;
        }
        let version = master.current_version;
        let seat_block = seat_sources_block(
            conn,
            retrieval,
            &session,
            &panel,
            &master,
            ROUND_ANSWER,
            &tuning,
        )?;
        let sources = join_blocks(&background_block, &seat_block);
        let layer = panel
            .layer_of(master_id)
            .unwrap_or_else(|| super::primary_layer(&master.layers));
        let request =
            independent_prompt_with_sources(&session.question, &master, &sources, layer);
        match call_model(conn, client, &request, policy) {
            Ok(response) => {
                save_ok(
                    conn,
                    session_id,
                    &panel,
                    ROUND_ANSWER,
                    "answer",
                    Some(master_id.clone()),
                    Some(version),
                    &response.content,
                )?;
                control::heartbeat(conn, session_id)?;
                answers.push((master.name.clone(), response.content));
                loaded.push(master);
                seat_blocks.push(sources);
            }
            Err(error) => {
                failed += 1;
                save_failed(
                    conn,
                    session_id,
                    &panel,
                    ROUND_ANSWER,
                    "answer",
                    Some(master_id.clone()),
                    Some(version),
                    &error,
                )?;
            }
        }
    }

    let answered = answers.len();
    if answered == 0 {
        if cancelled {
            control::mark_cancelled(conn, session_id, "", &[])?;
            return Ok(CouncilOutcome {
                session_id: session_id.to_string(),
                rotation: panel.rotation,
                answered,
                failed,
                conclusion: String::new(),
                divergences: Vec::new(),
                rounds: 0,
                metrics: Vec::new(),
            });
        }
        repo::update_status(conn, session_id, "failed")?;
        return Err(CoreError::ModelUnavailable {
            status: 0,
            message: "本次会诊全部席位模型调用失败".to_string(),
        });
    }

    // 交叉质询：第 2 轮起，按本轮分歧度决定是否追加，直到轮次上限或收敛。
    let mut history: Vec<Vec<(String, String)>> = vec![answers.clone()];
    // 已完成的质询轮从落库记录重建：断点续跑不重跑已完成轮次。
    let mut metrics: Vec<super::RoundMetric> = repo::metrics(conn, session_id, panel.rotation)?;
    let mut previous_divergence: Option<f64> = metrics.last().map(|metric| metric.divergence);
    let done_round = metrics
        .iter()
        .map(|metric| metric.round)
        .filter(|round| *round >= FIRST_CROSS_ROUND)
        .max()
        .unwrap_or(FIRST_CROSS_ROUND - 1);
    for completed in FIRST_CROSS_ROUND..=done_round {
        let pairs: Vec<(String, String)> = loaded
            .iter()
            .filter_map(|master| {
                ok_turns
                    .get(&(completed, master.id.clone()))
                    .map(|content| (master.name.clone(), content.clone()))
            })
            .collect();
        if !pairs.is_empty() {
            history.push(pairs);
        }
    }
    // 上一轮已判定收敛时不再追加，断点续跑直接进入收敛裁决。
    let converged_already = metrics.last().map(|metric| metric.converged).unwrap_or(false);
    let mut round = (done_round + 1).max(FIRST_CROSS_ROUND);

    while round <= max_rounds && !cancelled && !converged_already {
        if control::cancel_requested(conn, session_id)? {
            cancelled = true;
            break;
        }
        let mut critiques: Vec<(String, String)> = Vec::new();
        for (index, master) in loaded.iter().enumerate() {
            if control::cancel_requested(conn, session_id)? {
                cancelled = true;
                break;
            }
            if let Some(content) = ok_turns.get(&(round, master.id.clone())) {
                critiques.push((master.name.clone(), content.clone()));
                continue;
            }
            let sources = seat_blocks.get(index).map(String::as_str).unwrap_or("");
            let request = cross_prompt_with_sources(&session.question, master, &history, sources);
            match call_model(conn, client, &request, policy) {
                Ok(response) => {
                    save_ok(
                        conn,
                        session_id,
                        &panel,
                        round,
                        "cross",
                        Some(master.id.clone()),
                        Some(master.current_version),
                        &response.content,
                    )?;
                    control::heartbeat(conn, session_id)?;
                    critiques.push((master.name.clone(), response.content));
                }
                Err(error) => {
                    save_failed(
                        conn,
                        session_id,
                        &panel,
                        round,
                        "cross",
                        Some(master.id.clone()),
                        Some(master.current_version),
                        &error,
                    )?;
                }
            }
        }

        if critiques.is_empty() {
            // 本轮全部交叉质询都失败时，只剩失败记录，停止追加并保留已有指标。
            break;
        }

        let stats = super::divergence::round_metric(conn, judge, divergence_mode, &critiques)?;
        let divergence = stats.divergence;
        let converged = super::tuning::converged_of(
            divergence,
            previous_divergence,
            tuning.divergence_threshold,
            tuning.convergence_delta,
        );
        let metric = super::RoundMetric {
            session_id: session_id.to_string(),
            panel_rotation: panel.rotation,
            round,
            participant_count: stats.participant_count,
            avg_similarity: stats.avg_similarity,
            min_similarity: stats.min_similarity,
            divergence,
            converged,
            method: stats.method,
            fell_back: stats.fell_back,
            created_at: repo::now(conn)?,
        };
        repo::upsert_metric(conn, &metric)?;
        metrics.push(metric);

        history.push(critiques);
        previous_divergence = Some(divergence);

        let append = super::tuning::should_append(
            round,
            max_rounds,
            divergence,
            tuning.divergence_threshold,
            stats.participant_count as usize,
        );
        if converged || !append {
            break;
        }
        round += 1;
    }

    // 收敛裁决。断点续跑时复用已完成的裁决，不重复调用。
    let mut conclusion = existing
        .iter()
        .find(|turn| turn.role == "synthesis" && turn.status == "ok")
        .map(|turn| turn.content.clone())
        .unwrap_or_default();
    if conclusion.is_empty() && history.len() > 1 {
        let request = synthesis_prompt(&session.question, &history);
        match call_model(conn, client, &request, policy) {
            Ok(response) => {
                let synthesis_round = round + 1;
                save_ok(
                    conn,
                    session_id,
                    &panel,
                    synthesis_round,
                    "synthesis",
                    None,
                    None,
                    &response.content,
                )?;
                control::heartbeat(conn, session_id)?;
                conclusion = response.content;
            }
            Err(error) => {
                let synthesis_round = round + 1;
                save_failed(
                    conn,
                    session_id,
                    &panel,
                    synthesis_round,
                    "synthesis",
                    None,
                    None,
                    &error,
                )?;
            }
        }
    }

    // 分歧摘要以最后一个质询轮的发言为准，与最新判断一致。
    // 每条分歧都要能说出落在哪一题，所以把席位被指派到的题一并带上。
    let latest = history.last().cloned().unwrap_or_else(|| answers.clone());
    let seat_answers: Vec<SeatAnswer> = latest
        .iter()
        .map(|(name, content)| {
            let layer = loaded
                .iter()
                .find(|master| &master.name == name)
                .map(|master| {
                    panel
                        .layer_of(&master.id)
                        .unwrap_or_else(|| super::primary_layer(&master.layers))
                })
                .unwrap_or(Layer::Fa);
            SeatAnswer {
                name: name.clone(),
                content: content.clone(),
                layer,
            }
        })
        .collect();
    let divergences = derive_divergences(&seat_answers);
    if cancelled {
        control::mark_cancelled(conn, session_id, &conclusion, &divergences)?;
    } else {
        repo::finish_session(conn, session_id, &conclusion, &divergences)?;
    }

    Ok(CouncilOutcome {
        session_id: session_id.to_string(),
        rotation: panel.rotation,
        answered,
        failed,
        conclusion,
        divergences,
        rounds: history.len(),
        metrics,
    })
}

/// 某席位的补充检索资料块。未开启席位检索或未接入检索能力时返回空串。
fn seat_sources_block(
    conn: &Connection,
    retrieval: &Retrieval<'_>,
    session: &SessionView,
    panel: &PanelView,
    master: &MasterDetail,
    round: i64,
    tuning: &super::tuning::TuningSnapshot,
) -> CoreResult<String> {
    if !tuning.seat_search {
        return Ok(String::new());
    }
    let query = format!("{} {}", session.question, master.domain);
    let sources = connector_service::collect_for_seat(
        conn,
        retrieval,
        &session.id,
        panel.rotation,
        round,
        &master.id,
        &[query],
    )?;
    let fetched_at = sources
        .first()
        .map(|source| source.fetched_at.clone())
        .unwrap_or_default();
    Ok(connector_service::sources_block(
        "该席位补充检索",
        &sources,
        &fetched_at,
    ))
}

fn join_blocks(background: &str, seat: &str) -> String {
    match (background.trim().is_empty(), seat.trim().is_empty()) {
        (true, _) => seat.to_string(),
        (false, true) => background.to_string(),
        (false, false) => format!("{background}\n\n{seat}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn save_ok(
    conn: &Connection,
    session_id: &str,
    panel: &PanelView,
    round: i64,
    role: &str,
    master_id: Option<String>,
    master_version: Option<i64>,
    content: &str,
) -> CoreResult<String> {
    repo::save_turn(
        conn,
        &repo::NewTurn {
            session_id: session_id.to_string(),
            round,
            panel_rotation: panel.rotation,
            role: role.to_string(),
            master_id,
            master_version,
            content: content.to_string(),
            citations: Vec::new(),
            prompt_version: PROMPT_VERSION.to_string(),
            status: "ok".to_string(),
            error_code: None,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn save_failed(
    conn: &Connection,
    session_id: &str,
    panel: &PanelView,
    round: i64,
    role: &str,
    master_id: Option<String>,
    master_version: Option<i64>,
    error: &CoreError,
) -> CoreResult<String> {
    repo::save_turn(
        conn,
        &repo::NewTurn {
            session_id: session_id.to_string(),
            round,
            panel_rotation: panel.rotation,
            role: role.to_string(),
            master_id,
            master_version,
            content: String::new(),
            citations: Vec::new(),
            prompt_version: PROMPT_VERSION.to_string(),
            status: "failed".to_string(),
            error_code: Some(error.code().to_string()),
        },
    )
}
