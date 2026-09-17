//! 会诊会话仓储：会话、阵容轮次与发言记录的读写。

use rusqlite::{Connection, OptionalExtension};

use crate::error::{CoreError, CoreResult};
use crate::master::Layer;
use crate::util::unique_id;

use super::{
    DivergenceView, MasterHistoryEntry, PanelView, RoundMetric, SeatRef, Selection, SessionDetail,
    SessionView, Strategy, TurnView,
};

pub(crate) fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

fn json_array(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

fn layers_json(layers: &[Layer]) -> String {
    let names: Vec<&str> = layers.iter().map(|layer| layer.as_str()).collect();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
}

fn parse_layers(raw: &str) -> Vec<Layer> {
    let names: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    let mut layers: Vec<Layer> = names.iter().filter_map(|name| Layer::parse(name)).collect();
    layers.sort();
    layers
}

fn divergences_json(divergences: &[DivergenceView]) -> String {
    serde_json::to_string(divergences).unwrap_or_else(|_| "[]".to_string())
}

fn parse_strings(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// 读取分歧清单。升级前的历史会话存的是纯文本数组，按「法」这一题回填，
/// 保证旧会话仍能显示原有内容。
fn parse_divergences(raw: &str) -> Vec<DivergenceView> {
    if let Ok(items) = serde_json::from_str::<Vec<DivergenceView>>(raw) {
        return items;
    }
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|text| DivergenceView {
            layer: Layer::Fa,
            text,
        })
        .collect()
}

fn seats_json(seats: &[SeatRef]) -> String {
    serde_json::to_string(seats).unwrap_or_else(|_| "[]".to_string())
}

fn parse_seats(raw: &str) -> Vec<SeatRef> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// 历史阵容缺少席位指派时的回退：按大师声明层次里最靠抽象端的一层。
fn fallback_seat(conn: &Connection, master_id: &str) -> CoreResult<SeatRef> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT layers_json FROM masters WHERE id = ?1",
            [master_id],
            |row| row.get(0),
        )
        .optional()?;
    let layers = raw
        .as_deref()
        .map(parse_layers)
        .unwrap_or_default();
    Ok(SeatRef {
        master_id: master_id.to_string(),
        layer: super::primary_layer(&layers),
    })
}

fn parse_strategy(raw: &str) -> Strategy {
    Strategy::parse(raw).unwrap_or(Strategy::Steady)
}

/// 新建一次会诊，初始状态为草稿。
pub fn create_session(
    conn: &Connection,
    question: &str,
    domains: &[String],
    layers: &[Layer],
    strategy: Strategy,
) -> CoreResult<String> {
    let trimmed = question.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("会诊问题不能为空".into()));
    }
    let created_at = now(conn)?;
    let id = unique_id("council", trimmed);
    conn.execute(
        "INSERT INTO council_sessions
             (id, question, domains_json, layers_json, strategy, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'draft', ?6, ?6)",
        rusqlite::params![
            id,
            trimmed,
            json_array(domains),
            layers_json(layers),
            strategy.as_str(),
            created_at,
        ],
    )?;
    Ok(id)
}

/// 追问会话的落库参数。
pub struct NewFollowUp<'a> {
    pub parent_session_id: &'a str,
    pub anchor_kind: &'a str,
    pub anchor_text: &'a str,
    pub anchor_master_id: Option<&'a str>,
    pub anchor_round: Option<i64>,
    pub anchor_truncated: bool,
    pub panel_inherited: bool,
    pub question: &'a str,
    pub domains: &'a [String],
    pub layers: &'a [Layer],
    pub strategy: Strategy,
}

/// 新建一次追问会话，记录母会话与锚点。母会话不被改写。
pub fn create_followup_session(conn: &Connection, input: &NewFollowUp<'_>) -> CoreResult<String> {
    let created_at = now(conn)?;
    let id = unique_id("council", input.question);
    conn.execute(
        "INSERT INTO council_sessions
             (id, question, domains_json, layers_json, strategy, status, created_at, updated_at,
              parent_session_id, anchor_kind, anchor_text, anchor_master_id, anchor_round,
              anchor_truncated, panel_inherited)
         VALUES (?1, ?2, ?3, ?4, ?5, 'draft', ?6, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            id,
            input.question,
            json_array(input.domains),
            layers_json(input.layers),
            input.strategy.as_str(),
            created_at,
            input.parent_session_id,
            input.anchor_kind,
            input.anchor_text,
            input.anchor_master_id,
            input.anchor_round,
            i64::from(input.anchor_truncated),
            i64::from(input.panel_inherited),
        ],
    )?;
    Ok(id)
}

/// 把母会话某一次阵容原样复制到追问会话，使追问的推理依据与母会话一致。
pub fn copy_panel(
    conn: &Connection,
    source_session_id: &str,
    source_rotation: i64,
    target_session_id: &str,
    target_rotation: i64,
) -> CoreResult<bool> {
    let created_at = now(conn)?;
    let affected = conn.execute(
        "INSERT INTO council_panels
             (session_id, rotation, strategy, master_ids_json, pinned_ids_json, seats_json,
              layers_json, gaps_json, created_at)
         SELECT ?3, ?4, strategy, master_ids_json, pinned_ids_json, seats_json, layers_json,
                gaps_json, ?5
         FROM council_panels WHERE session_id = ?1 AND rotation = ?2",
        rusqlite::params![
            source_session_id,
            source_rotation,
            target_session_id,
            target_rotation,
            created_at,
        ],
    )?;
    Ok(affected > 0)
}

pub fn update_status(conn: &Connection, session_id: &str, status: &str) -> CoreResult<()> {
    let updated_at = now(conn)?;
    let affected = conn.execute(
        "UPDATE council_sessions SET status = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![session_id, status, updated_at],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("会诊 {session_id}")));
    }
    Ok(())
}

/// 记录本场是否包含「你」的席位。
pub fn set_self_seat_included(
    conn: &Connection,
    session_id: &str,
    included: bool,
) -> CoreResult<()> {
    let affected = conn.execute(
        "UPDATE council_sessions SET self_seat_included = ?2 WHERE id = ?1",
        rusqlite::params![session_id, i64::from(included)],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("会诊 {session_id}")));
    }
    Ok(())
}

/// 请求取消：仅在会话仍在运行时置位，已结束的会话按幂等处理。
pub fn request_cancel(conn: &Connection, session_id: &str) -> CoreResult<()> {
    let affected = conn.execute(
        "UPDATE council_sessions SET cancel_requested = 1
         WHERE id = ?1 AND status IN ('draft', 'running')",
        [session_id],
    )?;
    if affected == 0 {
        // 会话不存在时报错；已结束的会话保持原状。
        let _ = get_session(conn, session_id)?;
    }
    Ok(())
}

/// 取消是否已被请求。
pub fn cancel_requested(conn: &Connection, session_id: &str) -> CoreResult<bool> {
    let value: i64 = conn.query_row(
        "SELECT cancel_requested FROM council_sessions WHERE id = ?1",
        [session_id],
        |row| row.get(0),
    )?;
    Ok(value != 0)
}

/// 刷新心跳时刻。
pub fn heartbeat(conn: &Connection, session_id: &str) -> CoreResult<()> {
    let affected = conn.execute(
        "UPDATE council_sessions SET heartbeat_at = ?2 WHERE id = ?1",
        rusqlite::params![session_id, now(conn)?],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("会诊 {session_id}")));
    }
    Ok(())
}

/// 取消生效：置为已取消并记录时刻，已完成的轮次全部保留。
pub fn mark_cancelled(conn: &Connection, session_id: &str, conclusion: &str, divergences: &[DivergenceView]) -> CoreResult<()> {
    let affected = conn.execute(
        "UPDATE council_sessions
         SET status = 'cancelled', cancelled_at = ?2, conclusion = ?3, divergences_json = ?4,
             updated_at = ?2
         WHERE id = ?1",
        rusqlite::params![session_id, now(conn)?, conclusion, divergences_json(divergences)],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("会诊 {session_id}")));
    }
    Ok(())
}

/// 心跳早于给定秒数的运行中会话，按心跳时间升序。
pub fn recoverable(conn: &Connection, stale_seconds: i64) -> CoreResult<Vec<SessionView>> {
    let stale_seconds = stale_seconds.max(0);
    let mut stmt = conn.prepare(&format!(
        "{SESSION_SELECT}
         WHERE s.status = 'running'
           AND (s.heartbeat_at IS NULL
                OR s.heartbeat_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-' || ?1 || ' seconds'))
         ORDER BY s.heartbeat_at ASC, s.created_at ASC"
    ))?;
    let rows = stmt.query_map([stale_seconds], map_session)?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

/// 写入一次阵容轮次。换批时 rotation 递增，历史阵容全部保留。
pub fn record_panel(
    conn: &Connection,
    session_id: &str,
    rotation: i64,
    selection: &Selection,
    pinned: &[String],
) -> CoreResult<()> {
    let created_at = now(conn)?;
    let seats: Vec<SeatRef> = selection
        .seats
        .iter()
        .map(|seat| SeatRef {
            master_id: seat.master_id.clone(),
            layer: seat.layer,
        })
        .collect();
    conn.execute(
        "INSERT INTO council_panels
             (session_id, rotation, strategy, master_ids_json, pinned_ids_json, seats_json,
              layers_json, gaps_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (session_id, rotation) DO UPDATE SET
             strategy = excluded.strategy,
             master_ids_json = excluded.master_ids_json,
             pinned_ids_json = excluded.pinned_ids_json,
             seats_json = excluded.seats_json,
             layers_json = excluded.layers_json,
             gaps_json = excluded.gaps_json",
        rusqlite::params![
            session_id,
            rotation,
            selection.strategy.as_str(),
            json_array(&selection.master_ids()),
            json_array(pinned),
            seats_json(&seats),
            layers_json(&selection.layers),
            layers_json(&selection.gaps),
            created_at,
        ],
    )?;
    Ok(())
}

/// 当前阵容轮次数（最新 rotation）。
pub fn latest_rotation(conn: &Connection, session_id: &str) -> CoreResult<Option<i64>> {
    let value: Option<i64> = conn
        .query_row(
            "SELECT MAX(rotation) FROM council_panels WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(value)
}

pub fn panels(conn: &Connection, session_id: &str) -> CoreResult<Vec<PanelView>> {
    let mut stmt = conn.prepare(
        "SELECT rotation, strategy, master_ids_json, pinned_ids_json, layers_json, gaps_json,
                created_at, seats_json
         FROM council_panels WHERE session_id = ?1 ORDER BY rotation ASC",
    )?;
    let rows = stmt.query_map([session_id], |row| {
        Ok(PanelView {
            rotation: row.get(0)?,
            strategy: parse_strategy(&row.get::<_, String>(1)?),
            master_ids: parse_strings(&row.get::<_, String>(2)?),
            pinned_ids: parse_strings(&row.get::<_, String>(3)?),
            layers: parse_layers(&row.get::<_, String>(4)?),
            gaps: parse_layers(&row.get::<_, String>(5)?),
            created_at: row.get(6)?,
            seats: parse_seats(&row.get::<_, String>(7)?),
        })
    })?;
    let mut panels = Vec::new();
    for row in rows {
        let mut panel = row?;
        // 历史阵容没有席位指派时按大师层次回退，保证旧库仍可读。
        if panel.seats.len() != panel.master_ids.len() {
            panel.seats = panel
                .master_ids
                .iter()
                .map(|master_id| fallback_seat(conn, master_id))
                .collect::<CoreResult<Vec<_>>>()?;
        }
        panels.push(panel);
    }
    Ok(panels)
}

pub fn latest_panel(conn: &Connection, session_id: &str) -> CoreResult<Option<PanelView>> {
    Ok(panels(conn, session_id)?.into_iter().last())
}

/// 待写入的一轮发言。
pub struct NewTurn {
    pub session_id: String,
    pub round: i64,
    pub panel_rotation: i64,
    pub role: String,
    pub master_id: Option<String>,
    pub master_version: Option<i64>,
    pub content: String,
    pub citations: Vec<String>,
    /// 生成该轮发言时使用的提示词模板版本。
    pub prompt_version: String,
    pub status: String,
    pub error_code: Option<String>,
}

pub fn save_turn(conn: &Connection, turn: &NewTurn) -> CoreResult<String> {
    let created_at = now(conn)?;
    let id = unique_id(
        "turn",
        &format!("{}-{}-{}", turn.session_id, turn.round, turn.role),
    );
    conn.execute(
        "INSERT INTO council_turns
             (id, session_id, round, panel_rotation, role, master_id, master_version,
              content, citations_json, prompt_version, status, error_code, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            id,
            turn.session_id,
            turn.round,
            turn.panel_rotation,
            turn.role,
            turn.master_id,
            turn.master_version,
            turn.content,
            json_array(&turn.citations),
            turn.prompt_version,
            turn.status,
            turn.error_code,
            created_at,
        ],
    )?;
    Ok(id)
}

/// 读取发言记录，可按阵容轮次过滤。
pub fn turns(
    conn: &Connection,
    session_id: &str,
    rotation: Option<i64>,
) -> CoreResult<Vec<TurnView>> {
    let mut stmt = conn.prepare(
        "SELECT id, round, panel_rotation, role, master_id, master_version, content,
                citations_json, prompt_version, status, error_code, created_at
         FROM council_turns
         WHERE session_id = ?1 AND (?2 IS NULL OR panel_rotation = ?2)
         ORDER BY panel_rotation ASC, round ASC, created_at ASC, rowid ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![session_id, rotation], |row| {
        Ok(TurnView {
            id: row.get(0)?,
            round: row.get(1)?,
            panel_rotation: row.get(2)?,
            role: row.get(3)?,
            master_id: row.get(4)?,
            master_version: row.get(5)?,
            content: row.get(6)?,
            citations: parse_strings(&row.get::<_, String>(7)?),
            prompt_version: row.get(8)?,
            status: row.get(9)?,
            error_code: row.get(10)?,
            created_at: row.get(11)?,
        })
    })?;
    let mut turns = Vec::new();
    for row in rows {
        turns.push(row?);
    }
    Ok(turns)
}

/// 收敛裁决：写入结论与分歧点并置为已完成。
pub fn finish_session(
    conn: &Connection,
    session_id: &str,
    conclusion: &str,
    divergences: &[DivergenceView],
) -> CoreResult<()> {
    let updated_at = now(conn)?;
    let affected = conn.execute(
        "UPDATE council_sessions
         SET status = 'done', conclusion = ?2, divergences_json = ?3, updated_at = ?4
         WHERE id = ?1",
        rusqlite::params![session_id, conclusion, divergences_json(divergences), updated_at],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("会诊 {session_id}")));
    }
    Ok(())
}

fn map_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionView> {
    Ok(SessionView {
        id: row.get(0)?,
        question: row.get(1)?,
        domains: parse_strings(&row.get::<_, String>(2)?),
        layers: parse_layers(&row.get::<_, String>(3)?),
        strategy: parse_strategy(&row.get::<_, String>(4)?),
        status: row.get(5)?,
        conclusion: row.get(6)?,
        divergences: parse_divergences(&row.get::<_, String>(7)?),
        rotation_count: row.get(8)?,
        turn_count: row.get(9)?,
        parent_session_id: row.get(10)?,
        anchor_kind: row.get(11)?,
        anchor_text: row.get(12)?,
        anchor_master_id: row.get(13)?,
        anchor_round: row.get(14)?,
        anchor_truncated: row.get::<_, i64>(15)? != 0,
        panel_inherited: row.get::<_, i64>(16)? != 0,
        quota_policy: row.get(17)?,
        quota_max_rounds: row.get(18)?,
        quota_max_seats: row.get(19)?,
        quota_reason: row.get(20)?,
        self_seat_included: row.get::<_, i64>(21)? != 0,
        cancel_requested: row.get::<_, i64>(22)? != 0,
        cancelled_at: row.get(23)?,
        heartbeat_at: row.get(24)?,
        created_at: row.get(25)?,
        updated_at: row.get(26)?,
    })
}

const SESSION_SELECT: &str = "SELECT s.id, s.question, s.domains_json, s.layers_json, s.strategy,
        s.status, s.conclusion, s.divergences_json,
        (SELECT COUNT(*) FROM council_panels p WHERE p.session_id = s.id),
        (SELECT COUNT(*) FROM council_turns t WHERE t.session_id = s.id),
        s.parent_session_id, s.anchor_kind, s.anchor_text, s.anchor_master_id, s.anchor_round,
        s.anchor_truncated, s.panel_inherited,
        s.quota_policy, s.quota_max_rounds, s.quota_max_seats, s.quota_reason,
        s.self_seat_included, s.cancel_requested, s.cancelled_at, s.heartbeat_at,
        s.created_at, s.updated_at
     FROM council_sessions s";

/// 记录配额降级后的实际上下限，供结论页与回看展示。
pub fn set_quota(
    conn: &Connection,
    session_id: &str,
    policy: &str,
    max_rounds: Option<i64>,
    max_seats: Option<i64>,
    reason: &str,
) -> CoreResult<()> {
    let affected = conn.execute(
        "UPDATE council_sessions
         SET quota_policy = ?2, quota_max_rounds = ?3, quota_max_seats = ?4, quota_reason = ?5
         WHERE id = ?1",
        rusqlite::params![session_id, policy, max_rounds, max_seats, reason],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("会诊 {session_id}")));
    }
    Ok(())
}

pub fn get_session(conn: &Connection, session_id: &str) -> CoreResult<SessionView> {
    conn.query_row(
        &format!("{SESSION_SELECT} WHERE s.id = ?1"),
        [session_id],
        map_session,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("会诊 {session_id}")))
}

pub fn list_sessions(conn: &Connection, limit: i64) -> CoreResult<Vec<SessionView>> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(&format!(
        "{SESSION_SELECT} ORDER BY s.created_at DESC, s.rowid DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit], map_session)?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

pub fn session_detail(conn: &Connection, session_id: &str) -> CoreResult<SessionDetail> {
    let rotation = latest_rotation(conn, session_id)?.unwrap_or(0);
    Ok(SessionDetail {
        session: get_session(conn, session_id)?,
        panels: panels(conn, session_id)?,
        turns: turns(conn, session_id, None)?,
        metrics: metrics(conn, session_id, rotation)?,
    })
}

/// 写入或更新一轮指标。同一 (session, rotation, round) 只保留一条，重复写入只更新统计。
pub fn upsert_metric(conn: &Connection, metric: &RoundMetric) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO council_round_metrics
             (session_id, panel_rotation, round, participant_count, avg_similarity,
              min_similarity, divergence, converged, method, fell_back, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT (session_id, panel_rotation, round) DO UPDATE SET
             participant_count = excluded.participant_count,
             avg_similarity = excluded.avg_similarity,
             min_similarity = excluded.min_similarity,
             divergence = excluded.divergence,
             converged = excluded.converged,
             method = excluded.method,
             fell_back = excluded.fell_back",
        rusqlite::params![
            metric.session_id,
            metric.panel_rotation,
            metric.round,
            metric.participant_count,
            metric.avg_similarity,
            metric.min_similarity,
            metric.divergence,
            i64::from(metric.converged),
            metric.method,
            i64::from(metric.fell_back),
            metric.created_at,
        ],
    )?;
    Ok(())
}

/// 读取某一阵容轮次的分歧曲线，按轮次升序。换批后的曲线独立。
pub fn metrics(
    conn: &Connection,
    session_id: &str,
    rotation: i64,
) -> CoreResult<Vec<RoundMetric>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, panel_rotation, round, participant_count, avg_similarity,
                min_similarity, divergence, converged, method, fell_back, created_at
         FROM council_round_metrics
         WHERE session_id = ?1 AND panel_rotation = ?2
         ORDER BY round ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![session_id, rotation], |row| {
        Ok(RoundMetric {
            session_id: row.get(0)?,
            panel_rotation: row.get(1)?,
            round: row.get(2)?,
            participant_count: row.get(3)?,
            avg_similarity: row.get(4)?,
            min_similarity: row.get(5)?,
            divergence: row.get(6)?,
            converged: row.get::<_, i64>(7)? != 0,
            method: row.get(8)?,
            fell_back: row.get::<_, i64>(9)? != 0,
            created_at: row.get(10)?,
        })
    })?;
    let mut metrics = Vec::new();
    for row in rows {
        metrics.push(row?);
    }
    Ok(metrics)
}

/// 某位大师的入席历史：参与过的会诊与当时给出的判断。
pub fn master_history(
    conn: &Connection,
    master_id: &str,
    limit: i64,
) -> CoreResult<Vec<MasterHistoryEntry>> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT t.session_id, s.question, t.round, t.panel_rotation, t.role,
                t.master_version, t.content, t.created_at
         FROM council_turns t
         JOIN council_sessions s ON s.id = t.session_id
         WHERE t.master_id = ?1 AND t.status = 'ok'
         ORDER BY t.created_at DESC, t.rowid DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![master_id, limit], |row| {
        Ok(MasterHistoryEntry {
            session_id: row.get(0)?,
            question: row.get(1)?,
            round: row.get(2)?,
            panel_rotation: row.get(3)?,
            role: row.get(4)?,
            master_version: row.get(5)?,
            content: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    let mut history = Vec::new();
    for row in rows {
        history.push(row?);
    }
    Ok(history)
}
