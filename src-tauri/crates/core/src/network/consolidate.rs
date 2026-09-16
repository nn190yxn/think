//! 固化调度：把短期共激活沉淀为长期结构，并清理冗余与矛盾。
//!
//! 一次固化做四件事：强化本轮共激活的连线、衰减长期未激活的低权连线、
//! 合并内容高度相似的重复节点、识别同一对节点上支持与冲突并存的矛盾。
//! 四件事都以「已经处理过就不再重复处理」的方式写回，因此同一批输入
//! 重复固化不产生重复连线、重复节点或重复洞察。

use rusqlite::{Connection, OptionalExtension};

use crate::error::{CoreError, CoreResult};

use super::{
    cluster, ConsolidationReport, ConsolidationRunView, EDGE_STALE_DAYS, MAX_CONSOLIDATION_BATCH,
    STRENGTHEN_STEP,
};

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String = conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
        row.get(0)
    })?;
    Ok(value)
}

fn parse_mode(mode: &str) -> CoreResult<String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "manual" | "手动" => Ok("manual".to_string()),
        "idle" | "空闲" => Ok("idle".to_string()),
        other => Err(CoreError::InvalidInput(format!(
            "未知固化模式：{other}，只支持 manual 或 idle"
        ))),
    }
}

/// 触发一次固化并返回本次处理明细。
pub fn trigger_consolidation(conn: &Connection, mode: &str) -> CoreResult<ConsolidationReport> {
    let mode = parse_mode(mode)?;
    let started_at = now(conn)?;
    let run_id = crate::util::unique_id("consolidate", &format!("{mode}:{started_at}"));

    conn.execute(
        "INSERT INTO consolidation_runs (id, mode, started_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![run_id, mode, started_at],
    )?;

    let tuning = crate::council::tuning::snapshot(conn)?;

    let tx = conn.unchecked_transaction()?;
    let strengthened_count = strengthen_coactivated(&tx)?;
    let decayed_count = decay_stale_edges(&tx, tuning.min_edge_weight)?;
    let (merged_count, merged) = merge_similar_nodes(&tx, tuning.merge_similarity)?;
    let (conflict_count, conflicts) = detect_conflicts(&tx)?;
    let cluster_count = if tuning.cluster_enabled {
        let communities = cluster::detect(&tx, tuning.cluster_min_size as i64, tuning.min_edge_weight)?;
        cluster::persist(&tx, &run_id, &communities)?
    } else {
        0
    };
    tx.commit()?;

    let finished_at = now(conn)?;
    let report = ConsolidationReport {
        run_id: run_id.clone(),
        mode,
        strengthened_count,
        decayed_count,
        merged_count,
        conflict_count,
        cluster_count,
        merged,
        conflicts,
        started_at,
        finished_at,
    };
    let payload = serde_json::to_string(&report).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "UPDATE consolidation_runs
         SET finished_at = ?2, strengthened_count = ?3, decayed_count = ?4,
             merged_count = ?5, conflict_count = ?6, report_json = ?7
         WHERE id = ?1",
        rusqlite::params![
            run_id,
            report.finished_at,
            report.strengthened_count,
            report.decayed_count,
            report.merged_count,
            report.conflict_count,
            payload,
        ],
    )?;
    Ok(report)
}

/// 强化自上次固化以来被共同激活的连线，并把计数器清零以保持幂等。
fn strengthen_coactivated(conn: &Connection) -> CoreResult<i64> {
    let mut stmt = conn.prepare(
        "SELECT id, weight FROM thought_edges
         WHERE co_activation_count > 0 AND status != 'dropped'
         ORDER BY co_activation_count DESC, id ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map([MAX_CONSOLIDATION_BATCH], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;
    let mut edges = Vec::new();
    for row in rows {
        edges.push(row?);
    }
    drop(stmt);

    let count = edges.len() as i64;
    for (id, weight) in edges {
        conn.execute(
            "UPDATE thought_edges
             SET weight = ?2, co_activation_count = 0, status = 'active'
             WHERE id = ?1",
            rusqlite::params![id, (weight + STRENGTHEN_STEP).min(1.0)],
        )?;
    }
    Ok(count)
}

/// 把长期未激活且权重低于阈值的连线标记为陈旧，只做一次状态跃迁。
fn decay_stale_edges(conn: &Connection, min_edge_weight: f64) -> CoreResult<i64> {
    let cutoff = format!("-{EDGE_STALE_DAYS} days");
    let affected = conn.execute(
        "UPDATE thought_edges SET status = 'stale'
         WHERE status = 'active' AND weight < ?1
           AND (last_activated_at IS NULL
                OR last_activated_at < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?2))",
        rusqlite::params![min_edge_weight, cutoff],
    )?;
    Ok(affected as i64)
}

/// 合并同类型同来源且内容高度相似的节点，后者标记 superseded_by。
fn merge_similar_nodes(
    conn: &Connection,
    merge_similarity: f64,
) -> CoreResult<(i64, Vec<(String, String)>)> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, source_kind, normalized_content FROM thought_nodes
         WHERE superseded_by IS NULL
         ORDER BY created_at ASC, rowid ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map([MAX_CONSOLIDATION_BATCH], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut nodes = Vec::new();
    for row in rows {
        nodes.push(row?);
    }
    drop(stmt);

    let mut merged = Vec::new();
    let mut retired: Vec<usize> = Vec::new();
    for i in 0..nodes.len() {
        if retired.contains(&i) {
            continue;
        }
        for j in (i + 1)..nodes.len() {
            if retired.contains(&j) {
                continue;
            }
            let (keep_id, keep_kind, keep_source, keep_content) = &nodes[i];
            let (drop_id, drop_kind, drop_source, drop_content) = &nodes[j];
            if keep_kind != drop_kind || keep_source != drop_source {
                continue;
            }
            if similarity(keep_content, drop_content) < merge_similarity {
                continue;
            }
            repoint(conn, keep_id, drop_id)?;
            conn.execute(
                "UPDATE thought_nodes SET superseded_by = ?2 WHERE id = ?1",
                rusqlite::params![drop_id, keep_id],
            )?;
            merged.push((drop_id.clone(), keep_id.clone()));
            retired.push(j);
        }
    }
    Ok((merged.len() as i64, merged))
}

/// 把被合并节点的连线改指向保留节点。遇到唯一约束冲突时保留原状，
/// 该连线会因端点被取代而自然退出图谱，不产生重复边。
fn repoint(conn: &Connection, keep_id: &str, drop_id: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE OR IGNORE thought_edges SET from_node_id = ?2 WHERE from_node_id = ?1",
        rusqlite::params![drop_id, keep_id],
    )?;
    conn.execute(
        "UPDATE OR IGNORE thought_edges SET to_node_id = ?2 WHERE to_node_id = ?1",
        rusqlite::params![drop_id, keep_id],
    )?;
    Ok(())
}

/// 识别同一对节点上同时存在支持与冲突的连线，生成冲突洞察。
fn detect_conflicts(conn: &Connection) -> CoreResult<(i64, Vec<(String, String)>)> {
    let mut stmt = conn.prepare(
        "SELECT s.from_node_id, s.to_node_id FROM thought_edges s
         WHERE s.relation = 'supports' AND s.status = 'active'
           AND EXISTS (
               SELECT 1 FROM thought_edges c
               WHERE c.relation = 'conflicts' AND c.status = 'active'
                 AND ((c.from_node_id = s.from_node_id AND c.to_node_id = s.to_node_id)
                      OR (c.from_node_id = s.to_node_id AND c.to_node_id = s.from_node_id)))
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([MAX_CONSOLIDATION_BATCH], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        pairs.push(row?);
    }
    drop(stmt);

    let mut recorded = Vec::new();
    for (from, to) in pairs {
        let mut ids = [from, to];
        ids.sort();
        let related = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM insights WHERE kind = 'conflict' AND related_node_ids_json = ?1",
            [related.as_str()],
            |row| row.get(0),
        )?;
        if exists > 0 {
            continue;
        }
        let id = crate::util::unique_id("insight", &format!("conflict:{related}"));
        conn.execute(
            "INSERT INTO insights
                 (id, kind, title, summary, related_node_ids_json, status, created_at)
             VALUES (?1, 'conflict', '认知冲突', '同一对节点上同时存在支持与冲突的连线，需要裁决', ?2, 'new', ?3)",
            rusqlite::params![id, related, now(conn)?],
        )?;
        recorded.push((ids[0].clone(), ids[1].clone()));
    }
    Ok((recorded.len() as i64, recorded))
}

/// 归一化内容的相似度，取 1 - 编辑距离 / 较长长度。
fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    let longest = left.len().max(right.len());
    if longest == 0 {
        return 1.0;
    }
    1.0 - levenshtein(&left, &right) as f64 / longest as f64
}

fn levenshtein(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (i, &lc) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, &rc) in right.iter().enumerate() {
            let cost = usize::from(lc != rc);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

/// 读取一次固化的报告。历史记录缺少明细时用计数还原。
pub fn get_consolidation_report(conn: &Connection, run_id: &str) -> CoreResult<ConsolidationReport> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT report_json FROM consolidation_runs WHERE id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()?;
    let raw = raw.ok_or_else(|| CoreError::NotFound(format!("固化记录 {run_id}")))?;
    if let Ok(report) = serde_json::from_str::<ConsolidationReport>(&raw) {
        return Ok(report);
    }
    let run = get_run(conn, run_id)?;
    Ok(ConsolidationReport {
        run_id: run.id,
        mode: run.mode,
        strengthened_count: run.strengthened_count,
        decayed_count: run.decayed_count,
        merged_count: run.merged_count,
        conflict_count: run.conflict_count,
        cluster_count: 0,
        merged: Vec::new(),
        conflicts: Vec::new(),
        started_at: run.started_at,
        finished_at: run.finished_at.unwrap_or_default(),
    })
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConsolidationRunView> {
    Ok(ConsolidationRunView {
        id: row.get(0)?,
        mode: row.get(1)?,
        started_at: row.get(2)?,
        finished_at: row.get(3)?,
        strengthened_count: row.get(4)?,
        decayed_count: row.get(5)?,
        merged_count: row.get(6)?,
        conflict_count: row.get(7)?,
    })
}

const RUN_SELECT: &str = "SELECT id, mode, started_at, finished_at, strengthened_count,
        decayed_count, merged_count, conflict_count FROM consolidation_runs";

fn get_run(conn: &Connection, run_id: &str) -> CoreResult<ConsolidationRunView> {
    conn.query_row(&format!("{RUN_SELECT} WHERE id = ?1"), [run_id], map_run)
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("固化记录 {run_id}")))
}

/// 固化历史，按时间倒序。
pub fn list_consolidation_runs(
    conn: &Connection,
    limit: i64,
) -> CoreResult<Vec<ConsolidationRunView>> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(&format!(
        "{RUN_SELECT} ORDER BY started_at DESC, rowid DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit], map_run)?;
    let mut runs = Vec::new();
    for row in rows {
        runs.push(row?);
    }
    Ok(runs)
}
