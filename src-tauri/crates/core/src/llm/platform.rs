//! 模型平台配置仓储。
//!
//! 只存端点与模型名，密钥由外壳从系统凭据或用户环境读入，绝不落库。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

use super::openai::validate_endpoint;
use super::now;

/// 一个模型平台的配置视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformView {
    pub id: String,
    pub code: String,
    pub display_name: String,
    pub endpoint: String,
    pub model_name: String,
    /// 每千提示词 token 的单价，整数微元。
    pub input_price_micros_per_1k: i64,
    /// 每千补全 token 的单价，整数微元。
    pub output_price_micros_per_1k: i64,
    /// 计价币种，未填时按人民币展示。
    pub currency: String,
    pub enabled: bool,
    /// ready 表示可用，disabled 表示被用户关闭，unconfigured 表示端点或模型名不完整。
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 写入平台配置的输入。
#[derive(Debug, Clone)]
pub struct PlatformInput {
    pub code: String,
    pub display_name: String,
    pub endpoint: String,
    pub model_name: String,
    pub input_price_micros_per_1k: i64,
    pub output_price_micros_per_1k: i64,
    pub currency: String,
}

impl PlatformView {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlatformView> {
        Ok(PlatformView {
            id: row.get(0)?,
            code: row.get(1)?,
            display_name: row.get(2)?,
            endpoint: row.get(3)?,
            model_name: row.get(4)?,
            input_price_micros_per_1k: row.get(5)?,
            output_price_micros_per_1k: row.get(6)?,
            currency: row.get(7)?,
            enabled: row.get::<_, i64>(8)? != 0,
            status: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }
}

/// 平台状态由「是否启用」与「配置是否完整」共同决定。
fn status_of(enabled: bool, endpoint: &str, model_name: &str) -> &'static str {
    if endpoint.trim().is_empty() || model_name.trim().is_empty() {
        "unconfigured"
    } else if enabled {
        "ready"
    } else {
        "disabled"
    }
}

const COLUMNS: &str = "id, code, display_name, endpoint, model_name,
    input_price_micros_per_1k, output_price_micros_per_1k, currency,
    enabled, status, created_at, updated_at";

/// 按写入顺序列出全部平台。
pub fn list(conn: &Connection) -> CoreResult<Vec<PlatformView>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM ai_platforms ORDER BY created_at ASC, rowid ASC"
    ))?;
    let rows = stmt.query_map([], PlatformView::from_row)?;
    let mut platforms = Vec::new();
    for row in rows {
        platforms.push(row?);
    }
    Ok(platforms)
}

/// 读取单个平台。
pub fn get(conn: &Connection, code: &str) -> CoreResult<PlatformView> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM ai_platforms WHERE code = ?1"),
        [code],
        PlatformView::from_row,
    )
    .map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => {
            CoreError::NotFound(format!("模型平台不存在：{code}"))
        }
        other => other.into(),
    })
}

/// 读取当前启用的第一个平台。联网能力开启但未配置平台时返回 `None`。
pub fn enabled(conn: &Connection) -> CoreResult<Option<PlatformView>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM ai_platforms WHERE enabled = 1 AND status = 'ready'
         ORDER BY updated_at DESC, rowid ASC LIMIT 1"
    ))?;
    let mut rows = stmt.query_map([], PlatformView::from_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 新增或按 code 覆盖平台配置。启用状态由调用方通过 [`set_enabled`] 单独控制。
pub fn upsert(conn: &Connection, input: &PlatformInput) -> CoreResult<PlatformView> {
    let code = input.code.trim();
    if code.is_empty() {
        return Err(CoreError::InvalidInput("平台代码不能为空".to_string()));
    }
    if input.display_name.trim().is_empty() {
        return Err(CoreError::InvalidInput("平台名称不能为空".to_string()));
    }
    if !input.endpoint.trim().is_empty() {
        validate_endpoint(&input.endpoint)?;
    }
    if input.input_price_micros_per_1k < 0 || input.output_price_micros_per_1k < 0 {
        return Err(CoreError::InvalidInput("单价不能为负数".to_string()));
    }

    let endpoint = input.endpoint.trim();
    let model_name = input.model_name.trim();
    let display_name = input.display_name.trim();
    let currency = {
        let value = input.currency.trim();
        if value.is_empty() {
            "CNY".to_string()
        } else {
            value.to_uppercase()
        }
    };
    let now_value = now(conn)?;

    let existing: Option<(String, bool, String)> = conn
        .query_row(
            "SELECT id, enabled, created_at FROM ai_platforms WHERE code = ?1",
            [code],
            |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0, row.get(2)?)),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;

    let (id, enabled, created_at) = match existing {
        Some((id, enabled, created_at)) => (id, enabled, created_at),
        None => (unique_id("platform", code), false, now_value.clone()),
    };
    let status = status_of(enabled, endpoint, model_name);

    conn.execute(
        "INSERT INTO ai_platforms
             (id, code, display_name, endpoint, model_name,
              input_price_micros_per_1k, output_price_micros_per_1k, currency,
              enabled, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(code) DO UPDATE SET
             display_name = excluded.display_name,
             endpoint = excluded.endpoint,
             model_name = excluded.model_name,
             input_price_micros_per_1k = excluded.input_price_micros_per_1k,
             output_price_micros_per_1k = excluded.output_price_micros_per_1k,
             currency = excluded.currency,
             status = excluded.status,
             updated_at = excluded.updated_at",
        rusqlite::params![
            id,
            code,
            display_name,
            endpoint,
            model_name,
            input.input_price_micros_per_1k,
            input.output_price_micros_per_1k,
            currency,
            if enabled { 1 } else { 0 },
            status,
            created_at,
            now_value
        ],
    )?;

    get(conn, code)
}

/// 开启或关闭某个平台。配置不完整时不允许启用。
pub fn set_enabled(conn: &Connection, code: &str, enabled: bool) -> CoreResult<PlatformView> {
    let current = get(conn, code)?;
    if enabled && (current.endpoint.trim().is_empty() || current.model_name.trim().is_empty()) {
        return Err(CoreError::InvalidInput(
            "平台端点与模型名齐全后才能启用".to_string(),
        ));
    }
    let status = status_of(enabled, &current.endpoint, &current.model_name);
    let now = now(conn)?;
    conn.execute(
        "UPDATE ai_platforms SET enabled = ?2, status = ?3, updated_at = ?4 WHERE code = ?1",
        rusqlite::params![code, if enabled { 1 } else { 0 }, status, now],
    )?;
    get(conn, code)
}
