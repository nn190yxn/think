//! 回音检测与原则撤销。
//!
//! 自我大师包装的是用户过去的判断，长期在场会让相似结论被反复确认。这里把
//! 本次结论与既有个人原则做一次重合度比较，达到阈值时留下一条 `echo` 提示；
//! 原则撤销只改状态，不删节点，历史发言与连线保持不变。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

use super::{scoring, tuning};

/// 回音提示在 `insights` 里的类型标记。
pub const KIND_ECHO: &str = "echo";
/// 原则节点的类型标记。
pub const KIND_PRINCIPLE: &str = "principle";
/// 已撤销节点的状态。
pub const STATUS_REVOKED: &str = "revoked";
/// 仍在参与上下文的状态。
pub const STATUS_ACTIVE: &str = "active";

/// 一条与既有原则高度重合的命中。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EchoHit {
    pub node_id: String,
    pub content: String,
    pub overlap: f64,
}

/// 结论与既有原则的重合度比较。达到阈值的原则会留下一条 `echo` 提示记录。
pub fn detect_echo(conn: &Connection, judgment_id: &str) -> CoreResult<Vec<EchoHit>> {
    let content: String = conn
        .query_row(
            "SELECT content FROM thought_nodes WHERE id = ?1",
            [judgment_id],
            |row| row.get(0),
        )
        .map_err(|_| CoreError::NotFound(format!("节点 {judgment_id}")))?;
    let judgment_tokens = scoring::tokens(&content);

    let threshold = tuning::float_of(conn, "echo.threshold")?;
    let mut hits = Vec::new();
    for (node_id, principle) in active_principles(conn, 200)? {
        let overlap = scoring::overlap(&judgment_tokens, &scoring::tokens(&principle));
        if overlap >= threshold {
            record_hint(conn, judgment_id, &node_id, overlap)?;
        }
        hits.push(EchoHit {
            node_id,
            content: principle,
            overlap,
        });
    }
    hits.sort_by(|left, right| {
        right
            .overlap
            .partial_cmp(&left.overlap)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(hits)
}

/// 参与会诊上下文的原则：已撤销与已被取代的节点不在此列。
pub fn active_principles(conn: &Connection, limit: i64) -> CoreResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, content FROM thought_nodes
         WHERE kind = ?1 AND status = ?2 AND superseded_by IS NULL
         ORDER BY activation DESC, created_at DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![KIND_PRINCIPLE, STATUS_ACTIVE, limit.clamp(1, 500)],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut principles = Vec::new();
    for row in rows {
        principles.push(row?);
    }
    Ok(principles)
}

/// 撤销一条个人原则：保留内容，只置状态并记录原因与时刻。
pub fn revoke_principle(conn: &Connection, node_id: &str, reason: &str) -> CoreResult<()> {
    let (kind, already): (String, String) = conn
        .query_row(
            "SELECT kind, status FROM thought_nodes WHERE id = ?1",
            [node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| CoreError::NotFound(format!("原则 {node_id}")))?;
    if kind != KIND_PRINCIPLE {
        return Err(CoreError::InvalidInput(format!("节点 {node_id} 不是个人原则")));
    }
    if already == STATUS_REVOKED {
        return Ok(());
    }
    conn.execute(
        "UPDATE thought_nodes
         SET status = ?2, revoked_reason = ?3, revoked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?1",
        rusqlite::params![node_id, STATUS_REVOKED, reason.trim()],
    )?;
    Ok(())
}

/// 已撤销原则的留痕，内容与撤销原因仍可读。
pub fn revoked_principles(conn: &Connection, limit: i64) -> CoreResult<Vec<EchoHit>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, revoked_reason FROM thought_nodes
         WHERE kind = ?1 AND status = ?2
         ORDER BY revoked_at DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![KIND_PRINCIPLE, STATUS_REVOKED, limit.clamp(1, 500)],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let mut revoked = Vec::new();
    for row in rows {
        let (node_id, content, reason) = row?;
        revoked.push(EchoHit {
            node_id,
            content: format!("{content}\n撤销原因：{reason}"),
            overlap: 0.0,
        });
    }
    Ok(revoked)
}

/// 达到阈值的命中写一条 `echo` 提示；同一结论只留一条，重复检测不重复写。
fn record_hint(
    conn: &Connection,
    judgment_id: &str,
    node_id: &str,
    overlap: f64,
) -> CoreResult<()> {
    let existing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM insights
         WHERE kind = ?1 AND action = ?2 AND related_node_ids_json LIKE '%' || ?3 || '%'",
        rusqlite::params![KIND_ECHO, judgment_id, node_id],
        |row| row.get(0),
    )?;
    if existing > 0 {
        return Ok(());
    }
    let title: String = conn
        .query_row(
            "SELECT substr(content, 1, 40) FROM thought_nodes WHERE id = ?1",
            [node_id],
            |row| row.get(0),
        )
        .unwrap_or_default();
    conn.execute(
        "INSERT INTO insights
             (id, kind, title, summary, related_node_ids_json, related_master_ids_json,
              evidence_json, status, action, reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, '[]', ?6, 'new', ?7, ?8,
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        rusqlite::params![
            crate::util::unique_id("insight", &format!("echo-{judgment_id}")),
            KIND_ECHO,
            format!("本次结论与「{title}」高度重合"),
            "本次结论与你既有的某条原则高度重合，注意这可能是自我确认。",
            format!("[\"{node_id}\",\"{judgment_id}\"]"),
            format!("[{{\"overlap\":{overlap:.4}}}]"),
            judgment_id,
            "结论与既有原则重合度达到阈值",
        ],
    )?;
    Ok(())
}
