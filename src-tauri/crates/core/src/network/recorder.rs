//! 会诊落网：把一次会诊的结论、大师框架与分歧写进思维网络并强化连线。

use rusqlite::Connection;

use crate::council::repo as council_repo;
use crate::error::{CoreError, CoreResult};

use super::repo::{self, NewNode, NewRecord};
use super::{NodeKind, RecordOutcome, Relation, STRENGTHEN_STEP};

/// 会诊来源的标签，写入节点的 source_kind，便于回溯。
pub const SOURCE_RECORD: &str = "thought_record";
pub const SOURCE_TURN: &str = "council_turn";
pub const SOURCE_SESSION: &str = "council_session";

/// 框架指向判断的初始权重。
const FRAMEWORK_LINK_WEIGHT: f64 = 0.6;
/// 分歧指向判断的初始权重。
const DIVERGENCE_LINK_WEIGHT: f64 = 0.5;
/// 判断演化链上「新判断衍生自旧判断」的权重。
const PRIOR_LINK_WEIGHT: f64 = 0.7;
/// 一次会诊直接唤起的激活增量。
const RECORD_INCREMENT: f64 = 1.0;

/// 把一次已完成会诊写入思维网络。
///
/// 写入内容：判断节点取自结论，框架节点取自第一轮独立作答，分歧节点取自
/// 未收敛的分歧点；同主题的既有判断会与本次判断连成演化链并一并唤起。
pub fn record_session(conn: &Connection, session_id: &str) -> CoreResult<RecordOutcome> {
    let session = council_repo::get_session(conn, session_id)?;
    let conclusion = session.conclusion.trim();
    if conclusion.is_empty() {
        return Err(CoreError::InvalidInput(
            "会诊尚未收敛出结论，无法写入思维网络".into(),
        ));
    }

    let (record_id, topic_key) = repo::write_record(
        conn,
        &NewRecord {
            session_id: Some(session_id),
            question: &session.question,
            domains: &session.domains,
            layers: &session.layers,
            conclusion,
        },
    )?;

    let judgment = repo::upsert_node(
        conn,
        &NewNode {
            kind: NodeKind::Judgment,
            content: conclusion,
            source_kind: SOURCE_RECORD,
            source_ref: &record_id,
            domains: &session.domains,
            layers: &session.layers,
        },
    )?;

    // 第一轮独立作答代表各大师的框架；同一位大师重复发言时按来源合并。
    let turns = council_repo::turns(conn, session_id, None)?;
    let mut framework_ids = Vec::new();
    for turn in turns.iter().filter(|turn| turn.role == "answer" && turn.status == "ok") {
        let content = turn.content.trim();
        if content.is_empty() {
            continue;
        }
        let node = repo::upsert_node(
            conn,
            &NewNode {
                kind: NodeKind::Framework,
                content,
                source_kind: SOURCE_TURN,
                source_ref: &turn.id,
                domains: &session.domains,
                layers: &session.layers,
            },
        )?;
        if node.node_id == judgment.node_id {
            continue;
        }
        let edge = repo::link_nodes(
            conn,
            &node.node_id,
            &judgment.node_id,
            Relation::Derives,
            FRAMEWORK_LINK_WEIGHT,
        )?;
        strengthen(conn, &edge.edge_id)?;
        if !framework_ids.contains(&node.node_id) {
            framework_ids.push(node.node_id);
        }
    }

    let mut divergence_ids = Vec::new();
    for divergence in &session.divergences {
        let content = divergence.text.trim();
        if content.is_empty() {
            continue;
        }
        let node = repo::upsert_node(
            conn,
            &NewNode {
                kind: NodeKind::Question,
                content,
                source_kind: SOURCE_SESSION,
                source_ref: session_id,
                domains: &session.domains,
                // 分歧落在具体某一题上，节点只挂这一题，别再摊到全部层次。
                layers: std::slice::from_ref(&divergence.layer),
            },
        )?;
        if node.node_id == judgment.node_id {
            continue;
        }
        repo::link_nodes(
            conn,
            &node.node_id,
            &judgment.node_id,
            Relation::Conflicts,
            DIVERGENCE_LINK_WEIGHT,
        )?;
        if !divergence_ids.contains(&node.node_id) {
            divergence_ids.push(node.node_id);
        }
    }

    // 同主题的历史判断接成演化链：旧判断派生出新判断。
    let mut linked_prior = Vec::new();
    for prior_id in repo::prior_records(conn, &topic_key, &record_id)? {
        let Some(node_id) = repo::find_by_source(conn, SOURCE_RECORD, &prior_id)? else {
            continue;
        };
        // 结论内容未变时会命中同一个判断节点，此时无需自连。
        if node_id == judgment.node_id {
            continue;
        }
        repo::link_nodes(
            conn,
            &node_id,
            &judgment.node_id,
            Relation::Derives,
            PRIOR_LINK_WEIGHT,
        )?;
        linked_prior.push(node_id);
    }

    // 追问会话与母会话的问题不同，走独立的衍生连线，把原结论接到追问结论上。
    if let Some(parent_session_id) = session.parent_session_id.as_deref() {
        if let Some(parent_record_id) = repo::record_by_session(conn, parent_session_id)? {
            if let Some(parent_node_id) = repo::find_by_source(conn, SOURCE_RECORD, &parent_record_id)?
            {
                if parent_node_id != judgment.node_id && !linked_prior.contains(&parent_node_id) {
                    repo::link_nodes(
                        conn,
                        &parent_node_id,
                        &judgment.node_id,
                        Relation::Derives,
                        PRIOR_LINK_WEIGHT,
                    )?;
                    linked_prior.push(parent_node_id);
                }
            }
        }
    }

    let mut to_activate = vec![judgment.node_id.clone()];
    to_activate.extend(framework_ids.iter().cloned());
    to_activate.extend(divergence_ids.iter().cloned());
    let outcome = repo::activate(conn, &to_activate, RECORD_INCREMENT, Some(session_id))?;

    Ok(RecordOutcome {
        record_id,
        judgment_id: judgment.node_id,
        framework_ids,
        divergence_ids,
        linked_prior,
        activated: outcome.activated,
    })
}

/// 会诊产生的新连线在写入时即计入一次共激活，让首次固化能强化它。
fn strengthen(conn: &Connection, edge_id: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE thought_edges
         SET co_activation_count = co_activation_count + 1,
             last_activated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
             weight = MIN(1.0, weight + ?2)
         WHERE id = ?1",
        rusqlite::params![edge_id, STRENGTHEN_STEP],
    )?;
    Ok(())
}
