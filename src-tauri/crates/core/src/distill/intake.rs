//! 入库服务：手动投喂与主动搜集两条通道，共用待确认清单与重叠度评估。
//!
//! 主动搜集默认关闭；即使开启，新资料也必须逐批确认后才进入蒸馏。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

use super::pipeline::similarity;
use super::repo;
use super::{
    DiscoveryOutcome, IntakeJobView, IntakeMaterial, IntakeState, OverlapSummary, SignalStatus,
    SignalView, DUPLICATE_SIMILARITY_THRESHOLD, SOURCE_DISCOVERY, SOURCE_MANUAL,
};

/// 一条外部检索到的材料。真实检索由桌面外壳注入，核心只依赖该抽象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredMaterial {
    pub title: String,
    /// 公开地址或来源标识，用于去重。
    pub source_ref: String,
    #[serde(default)]
    pub summary: String,
}

/// 主动搜集的检索抽象。
pub trait DiscoveryClient {
    fn search(&self, query: &str) -> CoreResult<Vec<DiscoveredMaterial>>;
}

/// 未接入检索能力时的占位实现。
pub struct BlockedDiscovery {
    pub message: String,
}

impl BlockedDiscovery {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl DiscoveryClient for BlockedDiscovery {
    fn search(&self, _query: &str) -> CoreResult<Vec<DiscoveredMaterial>> {
        Err(CoreError::ModelUnavailable {
            status: 0,
            message: self.message.clone(),
        })
    }
}

/// 手动投喂入参。
#[derive(Debug, Clone)]
pub struct ManualIntakeInput {
    pub master_id: String,
    pub master_name: String,
    pub domain: String,
    pub materials: Vec<IntakeMaterial>,
}

/// 手动投喂：用户主动提供的材料直接进入队列，无需再确认。
pub fn create_manual(conn: &Connection, input: &ManualIntakeInput) -> CoreResult<IntakeJobView> {
    if input.materials.is_empty() {
        return Err(CoreError::InvalidInput("投喂至少需要一份材料".to_string()));
    }
    if input.materials.len() > super::MAX_MATERIALS {
        return Err(CoreError::InvalidInput(format!(
            "材料数量超过上限 {}",
            super::MAX_MATERIALS
        )));
    }
    let job = repo::create_intake(
        conn,
        &repo::NewIntakeJob {
            master_ref: &input.master_id,
            master_name: &input.master_name,
            domain: &input.domain,
            mode: SOURCE_MANUAL,
            state: IntakeState::Queued,
            schedule: serde_json::json!({}),
        },
    )?;

    let known = repo::known_sources(conn)?;
    let mut duplicates = 0i64;
    let mut max_ratio = 0.0f64;
    let mut saved = 0i64;
    for material in &input.materials {
        let ratio = overlap_ratio(&material.title, &material.text, &known);
        max_ratio = max_ratio.max(ratio);
        if ratio >= DUPLICATE_SIMILARITY_THRESHOLD {
            duplicates += 1;
        }
        repo::insert_signal(
            conn,
            &repo::NewSignal {
                job_id: &job.id,
                master_id: &input.master_id,
                title: &material.title,
                source_ref: &material.source_ref,
                kind: &material.kind,
                text: &material.text,
                overlap_ratio: ratio,
            },
        )?;
        saved += 1;
    }
    let accepted_ids: Vec<String> = repo::signals_of_job(conn, &job.id, None)?
        .into_iter()
        .map(|signal| signal.id)
        .collect();
    for id in &accepted_ids {
        repo::decide_signal(conn, id, SignalStatus::Accepted, "")?;
    }
    let overlap = OverlapSummary {
        materials: saved,
        duplicates,
        max_ratio,
        notes: overlap_notes(max_ratio, duplicates),
    };
    repo::set_intake_overlap(conn, &job.id, &overlap)?;
    repo::update_intake(conn, &job.id, IntakeState::Confirmed, saved, saved, 0)
}

/// 主动搜集：默认关闭。开启后检索结果进入待确认清单，不直接蒸馏。
pub fn run_discovery(
    conn: &Connection,
    client: &dyn DiscoveryClient,
    master_id: &str,
    master_name: &str,
    domain: &str,
    query: Option<&str>,
) -> CoreResult<DiscoveryOutcome> {
    let settings = repo::get_discovery_settings(conn)?;
    if !settings.enabled {
        return Ok(DiscoveryOutcome {
            triggered: false,
            reason: Some("disabled".to_string()),
            discovered: 0,
            saved: 0,
            pending: 0,
            job_id: None,
        });
    }

    let query = query
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{master_name} {domain} 方法论 观点"));
    let discovered = client.search(&query)?;

    let job = repo::create_intake(
        conn,
        &repo::NewIntakeJob {
            master_ref: master_id,
            master_name,
            domain,
            mode: SOURCE_DISCOVERY,
            state: IntakeState::AwaitingConfirmation,
            schedule: serde_json::json!({ "query": query }),
        },
    )?;

    let known = repo::known_sources(conn)?;
    let mut largest = 0.0f64;
    let mut saved = 0i64;
    let mut duplicates = 0i64;
    for material in &discovered {
        if material.source_ref.trim().is_empty() {
            continue;
        }
        if repo::signal_exists(conn, material.source_ref.trim())? {
            duplicates += 1;
            continue;
        }
        let ratio = overlap_ratio(&material.title, &material.summary, &known);
        largest = largest.max(ratio);
        repo::insert_signal(
            conn,
            &repo::NewSignal {
                job_id: &job.id,
                master_id,
                title: &material.title,
                source_ref: material.source_ref.trim(),
                kind: "link",
                text: &material.summary,
                overlap_ratio: ratio,
            },
        )?;
        saved += 1;
    }
    let overlap = OverlapSummary {
        materials: discovered.len() as i64,
        duplicates,
        max_ratio: largest,
        notes: overlap_notes(largest, duplicates),
    };
    repo::set_intake_overlap(conn, &job.id, &overlap)?;
    let pending = repo::signals_of_job(conn, &job.id, Some(SignalStatus::Pending.as_str()))?.len()
        as i64;

    Ok(DiscoveryOutcome {
        triggered: true,
        reason: None,
        discovered: discovered.len() as i64,
        saved,
        pending,
        job_id: Some(job.id),
    })
}

/// 待确认清单。
pub fn preview_materials(conn: &Connection, job_id: &str) -> CoreResult<Vec<SignalView>> {
    repo::get_intake(conn, job_id)?;
    let mut signals = repo::signals_of_job(conn, job_id, Some(SignalStatus::Pending.as_str()))?;
    if signals.is_empty() {
        signals = repo::signals_of_job(conn, job_id, None)?;
    }
    Ok(signals)
}

/// 逐批确认或拒绝。只有确认的材料会进入蒸馏。
pub fn confirm_materials(
    conn: &Connection,
    job_id: &str,
    accepted_ids: &[String],
    rejected_ids: &[String],
    reason: &str,
) -> CoreResult<IntakeJobView> {
    let job = repo::get_intake(conn, job_id)?;
    for id in accepted_ids {
        repo::decide_signal(conn, id, SignalStatus::Accepted, reason)?;
    }
    for id in rejected_ids {
        repo::decide_signal(conn, id, SignalStatus::Rejected, reason)?;
    }

    let signals = repo::signals_of_job(conn, job_id, None)?;
    let accepted = signals
        .iter()
        .filter(|signal| signal.status == SignalStatus::Accepted)
        .count() as i64;
    let rejected = signals
        .iter()
        .filter(|signal| signal.status == SignalStatus::Rejected)
        .count() as i64;
    let total = signals.len() as i64;
    let state = if accepted > 0 {
        IntakeState::Confirmed
    } else {
        IntakeState::Rejected
    };
    let _ = job;
    repo::update_intake(conn, job_id, state, total, accepted, rejected)
}

/// 已确认的材料，可供蒸馏流水线使用。
pub fn accepted_materials(conn: &Connection, job_id: &str) -> CoreResult<Vec<IntakeMaterial>> {
    Ok(repo::accepted_signals(conn, job_id)?
        .into_iter()
        .map(|signal| IntakeMaterial {
            title: signal.title,
            kind: signal.kind,
            source_ref: signal.source_ref,
            text: signal.text,
        })
        .collect())
}

/// 上次蒸馏被用户标记为不适用的判断，作为下次蒸馏的负面依据。
pub fn negative_evidence(conn: &Connection, master_id: &str) -> CoreResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT title, flagged_reason FROM master_units
         WHERE master_id = ?1 AND flagged_reason IS NOT NULL AND flagged_reason != ''
         ORDER BY title ASC",
    )?;
    let rows = stmt.query_map([master_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut notes = Vec::new();
    for row in rows {
        let (title, reason) = row?;
        notes.push(format!("{title}：{reason}"));
    }
    Ok(notes)
}

/// 重新计算某任务的资料重叠度。
pub fn evaluate_overlap(conn: &Connection, job_id: &str) -> CoreResult<OverlapSummary> {
    let signals = repo::signals_of_job(conn, job_id, None)?;
    let duplicates = signals
        .iter()
        .filter(|signal| signal.overlap_ratio >= DUPLICATE_SIMILARITY_THRESHOLD)
        .count() as i64;
    let max_ratio = signals
        .iter()
        .map(|signal| signal.overlap_ratio)
        .fold(0.0f64, f64::max);
    let summary = OverlapSummary {
        materials: signals.len() as i64,
        duplicates,
        max_ratio,
        notes: overlap_notes(max_ratio, duplicates),
    };
    repo::set_intake_overlap(conn, job_id, &summary)?;
    Ok(summary)
}

fn overlap_ratio(title: &str, text: &str, known: &[(String, String)]) -> f64 {
    let probe = format!("{title} {text}");
    known
        .iter()
        .map(|(source_ref, known_title)| {
            similarity(&probe, &format!("{known_title} {source_ref}"))
        })
        .fold(0.0f64, f64::max)
}

fn overlap_notes(max_ratio: f64, duplicates: i64) -> Vec<String> {
    let mut notes = Vec::new();
    if duplicates > 0 {
        notes.push(format!("{duplicates} 份资料与既有资料高度重叠，建议不重复蒸馏"));
    }
    if max_ratio >= DUPLICATE_SIMILARITY_THRESHOLD {
        notes.push(format!("最高重叠度 {:.0}%", max_ratio * 100.0));
    }
    notes
}
