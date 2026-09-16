//! 模型调用层：请求模型、调用审计与可注入的客户端抽象。
//!
//! 编排逻辑只依赖 [`ModelClient`] 抽象，因此离线环境下可以用脚本化客户端
//! 完整验证会诊链路，真实网络实现由桌面外壳注入。

pub mod openai;
pub mod platform;
pub mod probe;

use std::time::{Duration, Instant};

use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

/// 一次模型调用请求。
#[derive(Debug, Clone)]
pub struct ModelRequest {
    /// 调用用途，写入审计表便于区分会诊、蒸馏与后台碰撞。
    pub purpose: String,
    pub system: String,
    pub user: String,
    pub max_tokens: u32,
    /// 生成该请求的提示词模板版本，空串表示该调用与提示词版本无关。
    pub prompt_version: String,
}

impl ModelRequest {
    pub fn new(purpose: impl Into<String>, system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            purpose: purpose.into(),
            system: system.into(),
            user: user.into(),
            max_tokens: 1200,
            prompt_version: String::new(),
        }
    }

    /// 标注该请求使用的提示词模板版本，写入 `llm_calls` 供复现核对。
    pub fn with_prompt_version(mut self, version: impl Into<String>) -> Self {
        self.prompt_version = version.into();
        self
    }
}

/// 一次模型调用结果。
#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub content: String,
    pub platform: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

/// 模型客户端抽象。
pub trait ModelClient {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse>;
}

/// 联网能力关闭时拦截外部调用，保证本地优先语义。
pub struct GatedClient<'a> {
    pub enabled: bool,
    pub inner: &'a dyn ModelClient,
}

impl ModelClient for GatedClient<'_> {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse> {
        if !self.enabled {
            return Err(CoreError::NetworkOff(format!(
                "模型调用（{}）需要先开启模型平台联网能力",
                request.purpose
            )));
        }
        self.inner.complete(request)
    }
}

/// 重试策略。测试中把 `base_delay_ms` 设为 0 即可免等待。
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub base_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 3,
            base_delay_ms: 400,
        }
    }
}

/// 审计记录。
#[derive(Debug, Clone)]
pub struct CallRecord {
    pub purpose: String,
    pub platform_code: String,
    pub model_name: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// 本次调用的费用，整数微元。
    pub cost_micros: i64,
    pub latency_ms: i64,
    pub attempt: u32,
    pub status: String,
    pub error_code: Option<String>,
    pub prompt_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallView {
    pub id: String,
    pub purpose: String,
    pub platform_code: String,
    pub model_name: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub latency_ms: i64,
    pub attempt: i64,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

/// 写入一条调用审计。
pub fn record_call(conn: &Connection, record: &CallRecord) -> CoreResult<String> {
    let created_at = now(conn)?;
    let id = unique_id(
        "call",
        &format!("{}-{}-{}", record.purpose, record.attempt, created_at),
    );
    conn.execute(
        "INSERT INTO llm_calls
             (id, purpose, platform_code, model_name, prompt_tokens, completion_tokens,
              cost_micros, latency_ms, attempt, status, error_code, prompt_version, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            id,
            record.purpose,
            record.platform_code,
            record.model_name,
            record.prompt_tokens,
            record.completion_tokens,
            record.cost_micros,
            record.latency_ms,
            record.attempt,
            record.status,
            record.error_code,
            record.prompt_version,
            created_at,
        ],
    )?;
    Ok(id)
}

/// 调用模型并按指数退避重试，每一次尝试都写入审计。
pub fn call_model(
    conn: &Connection,
    client: &dyn ModelClient,
    request: &ModelRequest,
    policy: &RetryPolicy,
) -> CoreResult<ModelResponse> {
    let attempts = policy.attempts.max(1);
    let mut last_error: Option<CoreError> = None;

    for attempt in 1..=attempts {
        let started = Instant::now();
        match client.complete(request) {
            Ok(response) => {
                // 先按平台单价记账，再把同一笔费用写进调用审计，保证两处可加。
                let cost_micros = crate::cost::record_llm_cost(
                    conn,
                    response.prompt_tokens,
                    response.completion_tokens,
                    &response.platform,
                )?;
                record_call(
                    conn,
                    &CallRecord {
                        purpose: request.purpose.clone(),
                        platform_code: response.platform.clone(),
                        model_name: response.model.clone(),
                        prompt_tokens: response.prompt_tokens,
                        completion_tokens: response.completion_tokens,
                        cost_micros,
                        latency_ms: started.elapsed().as_millis() as i64,
                        attempt,
                        status: "ok".to_string(),
                        error_code: None,
                        prompt_version: request.prompt_version.clone(),
                    },
                )?;
                return Ok(response);
            }
            Err(error) => {
                record_call(
                    conn,
                    &CallRecord {
                        purpose: request.purpose.clone(),
                        platform_code: String::new(),
                        model_name: String::new(),
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        cost_micros: 0,
                        latency_ms: started.elapsed().as_millis() as i64,
                        attempt,
                        status: "failed".to_string(),
                        error_code: Some(error.code().to_string()),
                        prompt_version: request.prompt_version.clone(),
                    },
                )?;
                last_error = Some(error);
                if attempt < attempts && policy.base_delay_ms > 0 {
                    let delay = policy.base_delay_ms * 2u64.pow(attempt - 1);
                    std::thread::sleep(Duration::from_millis(delay));
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| CoreError::ModelUnavailable {
            status: 0,
            message: "模型调用失败".to_string(),
        }))
}

/// 最近的调用审计，供设置页与调试查看。
pub fn recent_calls(conn: &Connection, limit: i64) -> CoreResult<Vec<CallView>> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT id, purpose, platform_code, model_name, prompt_tokens, completion_tokens,
                latency_ms, attempt, status, error_code, created_at
         FROM llm_calls ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(CallView {
            id: row.get(0)?,
            purpose: row.get(1)?,
            platform_code: row.get(2)?,
            model_name: row.get(3)?,
            prompt_tokens: row.get(4)?,
            completion_tokens: row.get(5)?,
            latency_ms: row.get(6)?,
            attempt: row.get(7)?,
            status: row.get(8)?,
            error_code: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;
    let mut calls = Vec::new();
    for row in rows {
        calls.push(row?);
    }
    Ok(calls)
}

/// 某个用途最近一次调用审计的标识。
///
/// 自检探针需要把 `call_id` 回填给调用方，而 [`call_model`] 只返回响应体，
/// 因此这里按用途回查最后一条审计。
pub fn last_call_id(conn: &Connection, purpose: &str) -> CoreResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM llm_calls WHERE purpose = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [purpose],
            |row| row.get(0),
        )
        .optional()?)
}
