//! 连接器仓储：配置读写、调用审计与检索快照读写。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

use super::{is_known_kind, kind_label, normalize_snippet, status_of, ConnectorInput, ConnectorView};

/// 调用审计记录。
#[derive(Debug, Clone)]
pub struct ConnectorCallRecord {
    pub connector_id: Option<String>,
    pub kind: String,
    pub purpose: String,
    pub session_id: Option<String>,
    /// 用户提出的原始问句。
    pub query: String,
    /// 经过脱敏与模式转换后实际发出的串。
    pub query_sent: String,
    pub redacted: bool,
    pub result_count: i64,
    /// 本次调用的费用，整数微元。
    pub cost_micros: i64,
    pub latency_ms: i64,
    pub status: String,
    pub error_code: Option<String>,
}

/// 调用审计视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCallView {
    pub id: String,
    pub connector_id: Option<String>,
    pub kind: String,
    pub kind_label: String,
    pub purpose: String,
    pub session_id: Option<String>,
    pub query: String,
    pub query_original: String,
    pub query_sent: String,
    pub redacted: bool,
    pub result_count: i64,
    pub latency_ms: i64,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

/// 写入检索快照的输入。
#[derive(Debug, Clone)]
pub struct NewSource {
    pub session_id: String,
    pub panel_rotation: i64,
    pub round: i64,
    pub master_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
    pub fetched_at: String,
    pub body: Option<String>,
    /// 该条外部资料是否命中注入特征。
    pub flagged: bool,
}

pub(crate) fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

const COLUMNS: &str =
    "id, kind, display_name, endpoint, config_json, enabled, status, created_at, updated_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectorView> {
    let kind: String = row.get(1)?;
    let config_json: String = row.get(4)?;
    Ok(ConnectorView {
        id: row.get(0)?,
        kind_label: kind_label(&kind).to_string(),
        kind,
        display_name: row.get(2)?,
        endpoint: row.get(3)?,
        config: serde_json::from_str(&config_json).unwrap_or(serde_json::Value::Null),
        enabled: row.get::<_, i64>(5)? != 0,
        status: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// 按写入顺序列出全部连接器。
pub fn list(conn: &Connection) -> CoreResult<Vec<ConnectorView>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM connectors ORDER BY created_at ASC, rowid ASC"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    let mut connectors = Vec::new();
    for row in rows {
        connectors.push(row?);
    }
    Ok(connectors)
}

/// 读取单个连接器。
pub fn get(conn: &Connection, id: &str) -> CoreResult<ConnectorView> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM connectors WHERE id = ?1"),
        [id],
        from_row,
    )
    .map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => {
            CoreError::NotFound(format!("连接器不存在：{id}"))
        }
        other => other.into(),
    })
}

/// 读取第一个已启用的指定类型连接器。
pub fn enabled_of_kind(conn: &Connection, kind: &str) -> CoreResult<Option<ConnectorView>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM connectors
         WHERE kind = ?1 AND enabled = 1 AND status = 'ready'
         ORDER BY updated_at DESC, rowid ASC LIMIT 1"
    ))?;
    let mut rows = stmt.query_map([kind], from_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 新增或按 id 覆盖连接器配置。未提供 id 时按类型与名称生成。
pub fn upsert(conn: &Connection, input: &ConnectorInput) -> CoreResult<ConnectorView> {
    let kind = input.kind.trim();
    if !is_known_kind(kind) {
        return Err(CoreError::InvalidInput(format!("未知连接器类型：{kind}")));
    }
    if input.display_name.trim().is_empty() {
        return Err(CoreError::InvalidInput("连接器名称不能为空".to_string()));
    }
    if input.endpoint.trim().is_empty() {
        return Err(CoreError::InvalidInput("连接器地址不能为空".to_string()));
    }

    let display_name = input.display_name.trim();
    let endpoint = input.endpoint.trim();
    let config_json = serde_json::to_string(&input.config).unwrap_or_else(|_| "{}".to_string());
    let now_value = now(conn)?;

    let chosen = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let existing: Option<(String, bool, String)> = match chosen {
        Some(id) => conn
            .query_row(
                "SELECT id, enabled, created_at FROM connectors WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0, row.get(2)?)),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?,
        None => conn
            .query_row(
                "SELECT id, enabled, created_at FROM connectors WHERE kind = ?1 AND display_name = ?2",
                rusqlite::params![kind, display_name],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0, row.get(2)?)),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?,
    };

    let (id, enabled, created_at) = match existing {
        Some((id, enabled, created_at)) => (id, enabled, created_at),
        None => (
            unique_id("connector", &format!("{kind}-{display_name}")),
            false,
            now_value.clone(),
        ),
    };
    let status = status_of(enabled, endpoint);

    conn.execute(
        "INSERT INTO connectors
             (id, kind, display_name, endpoint, config_json, enabled, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
             kind = excluded.kind,
             display_name = excluded.display_name,
             endpoint = excluded.endpoint,
             config_json = excluded.config_json,
             status = excluded.status,
             updated_at = excluded.updated_at",
        rusqlite::params![
            id,
            kind,
            display_name,
            endpoint,
            config_json,
            if enabled { 1 } else { 0 },
            status,
            created_at,
            now_value
        ],
    )?;

    get(conn, &id)
}

/// 开启或关闭某个连接器。地址为空时不允许启用。
pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> CoreResult<ConnectorView> {
    let current = get(conn, id)?;
    if enabled && current.endpoint.trim().is_empty() {
        return Err(CoreError::InvalidInput(
            "连接器地址填写后才能启用".to_string(),
        ));
    }
    let status = status_of(enabled, &current.endpoint);
    let now = now(conn)?;
    conn.execute(
        "UPDATE connectors SET enabled = ?2, status = ?3, updated_at = ?4 WHERE id = ?1",
        rusqlite::params![id, if enabled { 1 } else { 0 }, status, now],
    )?;
    get(conn, id)
}

/// 写入一条调用审计。
pub fn record_call(conn: &Connection, record: &ConnectorCallRecord) -> CoreResult<String> {
    let created_at = now(conn)?;
    let id = unique_id(
        "conn-call",
        &format!("{}-{}-{}", record.purpose, record.kind, created_at),
    );
    conn.execute(
        "INSERT INTO connector_calls
             (id, connector_id, kind, purpose, session_id, query,
              query_original, query_sent, redacted, result_count,
              cost_micros, latency_ms, status, error_code, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            id,
            record.connector_id,
            record.kind,
            record.purpose,
            record.session_id,
            normalize_snippet(&record.query),
            normalize_snippet(&record.query),
            normalize_snippet(&record.query_sent),
            i64::from(record.redacted),
            record.result_count,
            record.cost_micros,
            record.latency_ms,
            record.status,
            record.error_code,
            created_at,
        ],
    )?;
    Ok(id)
}

/// 最近的连接器调用审计。
pub fn recent_calls(conn: &Connection, limit: i64) -> CoreResult<Vec<ConnectorCallView>> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT id, connector_id, kind, purpose, session_id, query,
                query_original, query_sent, redacted, result_count,
                latency_ms, status, error_code, created_at
         FROM connector_calls ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        let kind: String = row.get(2)?;
        Ok(ConnectorCallView {
            id: row.get(0)?,
            connector_id: row.get(1)?,
            kind_label: kind_label(&kind).to_string(),
            kind,
            purpose: row.get(3)?,
            session_id: row.get(4)?,
            query: row.get(5)?,
            query_original: row.get(6)?,
            query_sent: row.get(7)?,
            redacted: row.get::<_, i64>(8)? != 0,
            result_count: row.get(9)?,
            latency_ms: row.get(10)?,
            status: row.get(11)?,
            error_code: row.get(12)?,
            created_at: row.get(13)?,
        })
    })?;
    let mut calls = Vec::new();
    for row in rows {
        calls.push(row?);
    }
    Ok(calls)
}

/// 本次会诊某一轮次内已发生的成功检索次数，用于上限约束。
pub fn search_count(conn: &Connection, session_id: &str) -> CoreResult<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM connector_calls
         WHERE session_id = ?1 AND kind = 'search' AND status = 'ok'",
        [session_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// 某次会诊某个范围内是否已有快照，用于冻结本次检索结果。
pub fn has_sources(
    conn: &Connection,
    session_id: &str,
    rotation: i64,
    master_id: Option<&str>,
) -> CoreResult<bool> {
    let count: i64 = match master_id {
        Some(master_id) => conn.query_row(
            "SELECT COUNT(*) FROM council_sources
             WHERE session_id = ?1 AND panel_rotation = ?2 AND master_id = ?3",
            rusqlite::params![session_id, rotation, master_id],
            |row| row.get(0),
        )?,
        None => conn.query_row(
            "SELECT COUNT(*) FROM council_sources
             WHERE session_id = ?1 AND panel_rotation = ?2 AND master_id IS NULL",
            rusqlite::params![session_id, rotation],
            |row| row.get(0),
        )?,
    };
    Ok(count > 0)
}

/// 写入一条检索快照，返回其标识。
pub fn insert_source(conn: &Connection, source: &NewSource) -> CoreResult<String> {
    let created_at = now(conn)?;
    let id = unique_id(
        "source",
        &format!("{}-{}-{}", source.session_id, source.title, created_at),
    );
    conn.execute(
        "INSERT INTO council_sources
             (id, session_id, panel_rotation, round, master_id, kind, title, url,
              snippet, published_at, fetched_at, body, flagged, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            id,
            source.session_id,
            source.panel_rotation,
            source.round,
            source.master_id,
            source.kind,
            source.title,
            source.url,
            source.snippet,
            source.published_at,
            source.fetched_at,
            source.body,
            i64::from(source.flagged),
            created_at,
        ],
    )?;
    Ok(id)
}

/// 网页正文快照。
pub fn body(conn: &Connection, source_id: &str) -> CoreResult<Option<String>> {
    let body: Option<String> = conn.query_row(
        "SELECT body FROM council_sources WHERE id = ?1",
        [source_id],
        |row| row.get(0),
    )?;
    Ok(body)
}
