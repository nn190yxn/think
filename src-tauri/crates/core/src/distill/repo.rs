//! 蒸馏与入库仓储：任务、检查点、动态信号与主动搜集设置。

use rusqlite::{Connection, OptionalExtension};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

use super::{
    DistillDetail, DistillDraft, DistillJobView, DistillStage, DistillState, DiscoverySettings,
    IntakeJobView, IntakeMaterial, IntakeState, OverlapSummary, SignalStatus, SignalView,
};

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

/// 新建蒸馏任务的入参。
pub struct NewDistillJob<'a> {
    pub source_kind: &'a str,
    pub source_ref: &'a str,
    pub master_id: &'a str,
    pub master_name: &'a str,
    pub domain: &'a str,
    pub output_dir: &'a str,
    pub materials: &'a [IntakeMaterial],
    /// 上次蒸馏被标记为不适用的判断，作为本次的负面依据。
    pub negative: &'a [String],
}

const JOB_COLUMNS: &str = "id, source_kind, source_ref, master_id, master_name, domain, output_dir,\
 stage, state, error_code, model_calls, updated_at, created_at";

fn read_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<DistillJobView> {
    let stage_raw: String = row.get(7)?;
    let state_raw: String = row.get(8)?;
    let stage = DistillStage::parse(&stage_raw).unwrap_or(DistillStage::Skeleton);
    let state = DistillState::parse(&state_raw).unwrap_or(DistillState::Pending);
    Ok(DistillJobView {
        id: row.get(0)?,
        source_kind: row.get(1)?,
        source_ref: row.get(2)?,
        master_id: row.get(3)?,
        master_name: row.get(4)?,
        domain: row.get(5)?,
        output_dir: row.get(6)?,
        stage,
        stage_name: stage.name().to_string(),
        state,
        material_count: 0,
        model_calls: row.get(10)?,
        error_code: row.get(9)?,
        updated_at: row.get(11)?,
        created_at: row.get(12)?,
    })
}

/// 新建一个蒸馏任务，初始停在阶段0。
pub fn create_job(conn: &Connection, input: &NewDistillJob<'_>) -> CoreResult<DistillJobView> {
    let created_at = now(conn)?;
    let id = unique_id(
        "distill",
        &format!("{}-{}-{}", input.master_id, input.source_kind, created_at),
    );
    let materials_json = serde_json::to_string(input.materials)
        .map_err(|error| CoreError::InvalidInput(format!("材料无法序列化：{error}")))?;
    let negative_json = serde_json::to_string(input.negative)
        .map_err(|error| CoreError::InvalidInput(format!("负面依据无法序列化：{error}")))?;
    conn.execute(
        "INSERT INTO distill_jobs
             (id, source_kind, source_ref, master_id, master_name, domain, output_dir,
              materials_json, negative_json, stage, state, checkpoint_json, model_calls,
              updated_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'skeleton', 'pending', '{}', 0, ?10, ?10)",
        rusqlite::params![
            id,
            input.source_kind,
            input.source_ref,
            input.master_id,
            input.master_name,
            input.domain,
            input.output_dir,
            materials_json,
            negative_json,
            created_at,
        ],
    )?;
    get_job(conn, &id)
}

pub fn get_job(conn: &Connection, job_id: &str) -> CoreResult<DistillJobView> {
    let sql = format!("SELECT {JOB_COLUMNS} FROM distill_jobs WHERE id = ?1");
    let mut job = conn
        .query_row(&sql, [job_id], read_job)
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("蒸馏任务 {job_id}")))?;
    job.material_count = job_materials(conn, job_id)?.len() as i64;
    Ok(job)
}

/// 列出蒸馏任务，`state` 为空时返回全部。
pub fn list_jobs(
    conn: &Connection,
    state: Option<&str>,
    limit: i64,
) -> CoreResult<Vec<DistillJobView>> {
    let limit = limit.clamp(1, 200);
    let sql = format!(
        "SELECT {JOB_COLUMNS} FROM distill_jobs
         WHERE (?1 IS NULL OR state = ?1)
         ORDER BY updated_at DESC, created_at DESC LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![state, limit], read_job)?;
    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(row?);
    }
    Ok(jobs)
}

/// 读取任务的检查点草稿。
pub fn get_draft(conn: &Connection, job_id: &str) -> CoreResult<DistillDraft> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT checkpoint_json FROM distill_jobs WHERE id = ?1",
            [job_id],
            |row| row.get(0),
        )
        .optional()?;
    let raw = raw.ok_or_else(|| CoreError::NotFound(format!("蒸馏任务 {job_id}")))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn get_detail(conn: &Connection, job_id: &str) -> CoreResult<DistillDetail> {
    Ok(DistillDetail {
        job: get_job(conn, job_id)?,
        draft: get_draft(conn, job_id)?,
    })
}

/// 读取任务保存的材料。
pub fn job_materials(conn: &Connection, job_id: &str) -> CoreResult<Vec<IntakeMaterial>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT materials_json FROM distill_jobs WHERE id = ?1",
            [job_id],
            |row| row.get(0),
        )
        .optional()?;
    let raw = raw.ok_or_else(|| CoreError::NotFound(format!("蒸馏任务 {job_id}")))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// 读取任务保存的负面依据。
pub fn job_negative(conn: &Connection, job_id: &str) -> CoreResult<Vec<String>> {
    let raw: String = conn
        .query_row(
            "SELECT negative_json FROM distill_jobs WHERE id = ?1",
            [job_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("蒸馏任务 {job_id}")))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// 写入检查点并推进阶段。三个阶段字段在同一事务内更新，避免半推进状态。
pub fn save_progress(
    conn: &Connection,
    job_id: &str,
    stage: DistillStage,
    state: DistillState,
    draft: &DistillDraft,
    error_code: Option<&str>,
    model_calls_delta: i64,
) -> CoreResult<()> {
    let checkpoint = serde_json::to_string(draft)
        .map_err(|error| CoreError::InvalidInput(format!("检查点无法序列化：{error}")))?;
    let updated_at = now(conn)?;
    let changed = conn.execute(
        "UPDATE distill_jobs
         SET stage = ?2, state = ?3, checkpoint_json = ?4, error_code = ?5,
             model_calls = model_calls + ?6, updated_at = ?7
         WHERE id = ?1",
        rusqlite::params![
            job_id,
            stage.as_str(),
            state.as_str(),
            checkpoint,
            error_code,
            model_calls_delta,
            updated_at,
        ],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("蒸馏任务 {job_id}")));
    }
    Ok(())
}

/// 只更新状态与错误码，阶段与草稿保持不变。
pub fn set_state(
    conn: &Connection,
    job_id: &str,
    state: DistillState,
    error_code: Option<&str>,
) -> CoreResult<()> {
    let updated_at = now(conn)?;
    let changed = conn.execute(
        "UPDATE distill_jobs SET state = ?2, error_code = ?3, updated_at = ?4 WHERE id = ?1",
        rusqlite::params![job_id, state.as_str(), error_code, updated_at],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("蒸馏任务 {job_id}")));
    }
    Ok(())
}

// ---------- 动态信号 ----------

pub struct NewSignal<'a> {
    pub job_id: &'a str,
    pub master_id: &'a str,
    pub title: &'a str,
    pub source_ref: &'a str,
    pub kind: &'a str,
    pub text: &'a str,
    pub overlap_ratio: f64,
}

const SIGNAL_COLUMNS: &str = "id, job_id, master_id, title, source_ref, kind, text, status,\
 decision_reason, overlap_ratio, discovered_at";

fn read_signal(row: &rusqlite::Row<'_>) -> rusqlite::Result<SignalView> {
    let status_raw: String = row.get(7)?;
    let status = match status_raw.as_str() {
        "accepted" => SignalStatus::Accepted,
        "rejected" => SignalStatus::Rejected,
        _ => SignalStatus::Pending,
    };
    Ok(SignalView {
        id: row.get(0)?,
        job_id: row.get(1)?,
        master_id: row.get(2)?,
        title: row.get(3)?,
        source_ref: row.get(4)?,
        kind: row.get(5)?,
        text: row.get(6)?,
        status,
        decision_reason: row.get(8)?,
        overlap_ratio: row.get(9)?,
        discovered_at: row.get(10)?,
    })
}

pub fn insert_signal(conn: &Connection, input: &NewSignal<'_>) -> CoreResult<SignalView> {
    let discovered_at = now(conn)?;
    let id = unique_id(
        "signal",
        &format!("{}-{}-{}", input.job_id, input.source_ref, discovered_at),
    );
    conn.execute(
        "INSERT INTO signals
             (id, job_id, master_id, title, source_ref, kind, text, status,
              decision_reason, overlap_ratio, discovered_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', '', ?8, ?9)",
        rusqlite::params![
            id,
            input.job_id,
            input.master_id,
            input.title,
            input.source_ref,
            input.kind,
            input.text,
            input.overlap_ratio,
            discovered_at,
        ],
    )?;
    get_signal(conn, &id)
}

pub fn get_signal(conn: &Connection, signal_id: &str) -> CoreResult<SignalView> {
    let sql = format!("SELECT {SIGNAL_COLUMNS} FROM signals WHERE id = ?1");
    conn.query_row(&sql, [signal_id], read_signal)
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("动态信号 {signal_id}")))
}

/// 列出某任务的信号，`status` 为空时返回全部。
pub fn signals_of_job(
    conn: &Connection,
    job_id: &str,
    status: Option<&str>,
) -> CoreResult<Vec<SignalView>> {
    let sql = format!(
        "SELECT {SIGNAL_COLUMNS} FROM signals
         WHERE job_id = ?1 AND (?2 IS NULL OR status = ?2)
         ORDER BY discovered_at ASC, id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![job_id, status], read_signal)?;
    let mut signals = Vec::new();
    for row in rows {
        signals.push(row?);
    }
    Ok(signals)
}

/// 已确认的信号，可作为蒸馏材料。
pub fn accepted_signals(conn: &Connection, job_id: &str) -> CoreResult<Vec<SignalView>> {
    signals_of_job(conn, job_id, Some(SignalStatus::Accepted.as_str()))
}

/// 判断来源是否已登记，用于搜集去重。
pub fn signal_exists(conn: &Connection, source_ref: &str) -> CoreResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM signals WHERE source_ref = ?1 AND status != 'rejected'",
        [source_ref],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// 记录一条信号的处置结果。
pub fn decide_signal(
    conn: &Connection,
    signal_id: &str,
    status: SignalStatus,
    reason: &str,
) -> CoreResult<SignalView> {
    let changed = conn.execute(
        "UPDATE signals SET status = ?2, decision_reason = ?3 WHERE id = ?1",
        rusqlite::params![signal_id, status.as_str(), reason],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("动态信号 {signal_id}")));
    }
    get_signal(conn, signal_id)
}

/// 全部未淘汰信号的来源地址，供重叠度评估。
pub fn known_sources(conn: &Connection) -> CoreResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT source_ref, title FROM signals WHERE status != 'rejected'",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut sources = Vec::new();
    for row in rows {
        sources.push(row?);
    }
    Ok(sources)
}

// ---------- 入库任务 ----------

pub struct NewIntakeJob<'a> {
    pub master_ref: &'a str,
    pub master_name: &'a str,
    pub domain: &'a str,
    pub mode: &'a str,
    pub state: IntakeState,
    pub schedule: serde_json::Value,
}

const INTAKE_COLUMNS: &str = "id, master_ref, master_name, domain, mode, state, material_count,\
 accepted_count, rejected_count, overlap_summary_json, updated_at, created_at";

fn read_intake(row: &rusqlite::Row<'_>) -> rusqlite::Result<IntakeJobView> {
    let state_raw: String = row.get(5)?;
    let overlap_raw: String = row.get(9)?;
    Ok(IntakeJobView {
        id: row.get(0)?,
        master_ref: row.get(1)?,
        master_name: row.get(2)?,
        domain: row.get(3)?,
        mode: row.get(4)?,
        state: IntakeState::parse(&state_raw).unwrap_or(IntakeState::Queued),
        material_count: row.get(6)?,
        accepted_count: row.get(7)?,
        rejected_count: row.get(8)?,
        overlap: serde_json::from_str(&overlap_raw).unwrap_or_default(),
        updated_at: row.get(10)?,
        created_at: row.get(11)?,
    })
}

pub fn create_intake(conn: &Connection, input: &NewIntakeJob<'_>) -> CoreResult<IntakeJobView> {
    let created_at = now(conn)?;
    let id = unique_id(
        "intake",
        &format!("{}-{}-{}", input.master_ref, input.mode, created_at),
    );
    let schedule = serde_json::to_string(&input.schedule).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT INTO intake_jobs
             (id, master_ref, master_name, domain, mode, state, material_count,
              accepted_count, rejected_count, overlap_summary_json, schedule_json,
              updated_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, 0, '{}', ?7, ?8, ?8)",
        rusqlite::params![
            id,
            input.master_ref,
            input.master_name,
            input.domain,
            input.mode,
            input.state.as_str(),
            schedule,
            created_at,
        ],
    )?;
    get_intake(conn, &id)
}

pub fn get_intake(conn: &Connection, job_id: &str) -> CoreResult<IntakeJobView> {
    let sql = format!("SELECT {INTAKE_COLUMNS} FROM intake_jobs WHERE id = ?1");
    conn.query_row(&sql, [job_id], read_intake)
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("入库任务 {job_id}")))
}

pub fn list_intake(
    conn: &Connection,
    state: Option<&str>,
    limit: i64,
) -> CoreResult<Vec<IntakeJobView>> {
    let limit = limit.clamp(1, 200);
    let sql = format!(
        "SELECT {INTAKE_COLUMNS} FROM intake_jobs
         WHERE (?1 IS NULL OR state = ?1)
         ORDER BY updated_at DESC, created_at DESC LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![state, limit], read_intake)?;
    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(row?);
    }
    Ok(jobs)
}

/// 更新入库任务的状态与计数。
pub fn update_intake(
    conn: &Connection,
    job_id: &str,
    state: IntakeState,
    material_count: i64,
    accepted_count: i64,
    rejected_count: i64,
) -> CoreResult<IntakeJobView> {
    let updated_at = now(conn)?;
    let changed = conn.execute(
        "UPDATE intake_jobs
         SET state = ?2, material_count = ?3, accepted_count = ?4, rejected_count = ?5,
             updated_at = ?6
         WHERE id = ?1",
        rusqlite::params![
            job_id,
            state.as_str(),
            material_count,
            accepted_count,
            rejected_count,
            updated_at,
        ],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("入库任务 {job_id}")));
    }
    get_intake(conn, job_id)
}

/// 写入重叠度评估结果。
pub fn set_intake_overlap(
    conn: &Connection,
    job_id: &str,
    overlap: &OverlapSummary,
) -> CoreResult<()> {
    let raw = serde_json::to_string(overlap)
        .map_err(|error| CoreError::InvalidInput(format!("重叠度无法序列化：{error}")))?;
    let updated_at = now(conn)?;
    let changed = conn.execute(
        "UPDATE intake_jobs SET overlap_summary_json = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![job_id, raw, updated_at],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("入库任务 {job_id}")));
    }
    Ok(())
}

/// 记录主动搜集计划。
pub fn set_intake_schedule(
    conn: &Connection,
    job_id: &str,
    schedule: &serde_json::Value,
) -> CoreResult<IntakeJobView> {
    let raw = serde_json::to_string(schedule).unwrap_or_else(|_| "{}".to_string());
    let updated_at = now(conn)?;
    let changed = conn.execute(
        "UPDATE intake_jobs SET schedule_json = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![job_id, raw, updated_at],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("入库任务 {job_id}")));
    }
    get_intake(conn, job_id)
}

// ---------- 主动搜集设置 ----------

pub fn get_discovery_settings(conn: &Connection) -> CoreResult<DiscoverySettings> {
    let row = conn
        .query_row(
            "SELECT enabled, schedule_json, updated_at FROM discovery_settings WHERE id = 'default'",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    match row {
        Some((enabled, schedule, updated_at)) => Ok(DiscoverySettings {
            enabled: enabled != 0,
            schedule: serde_json::from_str(&schedule).unwrap_or_else(|_| serde_json::json!({})),
            updated_at,
        }),
        None => Ok(DiscoverySettings {
            enabled: false,
            schedule: serde_json::json!({}),
            updated_at: String::new(),
        }),
    }
}

/// 主动搜集总开关。默认关闭，开启后仍需逐批确认。
pub fn set_discovery_enabled(
    conn: &Connection,
    enabled: bool,
) -> CoreResult<DiscoverySettings> {
    let updated_at = now(conn)?;
    conn.execute(
        "INSERT INTO discovery_settings (id, enabled, schedule_json, updated_at)
         VALUES ('default', ?1, '{}', ?2)
         ON CONFLICT(id) DO UPDATE SET enabled = ?1, updated_at = ?2",
        rusqlite::params![enabled as i64, updated_at],
    )?;
    get_discovery_settings(conn)
}

pub fn set_discovery_schedule(
    conn: &Connection,
    schedule: &serde_json::Value,
) -> CoreResult<DiscoverySettings> {
    let updated_at = now(conn)?;
    let raw = serde_json::to_string(schedule).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT INTO discovery_settings (id, enabled, schedule_json, updated_at)
         VALUES ('default', 0, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET schedule_json = ?1, updated_at = ?2",
        rusqlite::params![raw, updated_at],
    )?;
    get_discovery_settings(conn)
}
