//! 模型链路自检探针：用一次最小调用判定链路是否可用。
//!
//! 探针自身不把链路问题当成 `Err`：联网关闭、平台缺失与调用失败都编码进
//! [`ProbeOutcome`] 并带出退出码，只有数据库层面的错误才向上抛，便于外壳
//! 与命令行按同一份结果判定。

use std::time::Instant;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::CoreResult;

use super::platform::PlatformView;
use super::{call_model, last_call_id, ModelClient, ModelRequest, RetryPolicy};

/// 探针用途，写入 `llm_calls.purpose`。
pub const PROBE_PURPOSE: &str = "model_probe";
/// 探针提示词版本，改动探针提示词时必须递增。
pub const PROBE_PROMPT_VERSION: &str = "2026-09-15.1";
/// 固定探针提示词：以最小成本确认链路可用。
pub const PROBE_PROMPT: &str = "只回复两个字：可用";

/// 退出码：探针成功。
pub const EXIT_OK: i64 = 0;
/// 退出码：模型不可用。
pub const EXIT_MODEL_UNAVAILABLE: i64 = 1;
/// 退出码：联网能力关闭。
pub const EXIT_NETWORK_OFF: i64 = 2;
/// 退出码：平台未配置。
pub const EXIT_PLATFORM_MISSING: i64 = 3;

/// 一次探针的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutcome {
    pub ok: bool,
    /// 与设计约定一致的退出码，命令行自检据此判定。
    pub exit_code: i64,
    pub platform_code: String,
    pub model_name: String,
    pub latency_ms: i64,
    /// 本次探针的调用审计标识，未发起调用时为空串。
    pub call_id: String,
    /// 探针返回的正文，成功时通常为「可用」。
    pub snippet: String,
    pub error_code: Option<String>,
    /// 是否解析到可用密钥：只报存在与否，不回传任何密钥内容。
    pub key_present: bool,
}

/// 按错误码映射退出码。
fn exit_code_of(error_code: &str) -> i64 {
    match error_code {
        "E_NETWORK_OFF" => EXIT_NETWORK_OFF,
        "E_NOT_FOUND" => EXIT_PLATFORM_MISSING,
        _ => EXIT_MODEL_UNAVAILABLE,
    }
}

fn blocked(
    platform_code: String,
    model_name: String,
    error_code: &str,
    key_present: bool,
) -> ProbeOutcome {
    ProbeOutcome {
        ok: false,
        exit_code: exit_code_of(error_code),
        platform_code,
        model_name,
        latency_ms: 0,
        call_id: String::new(),
        snippet: String::new(),
        error_code: Some(error_code.to_string()),
        key_present,
    }
}

/// 用固定提示词做一次最小调用，写入审计并返回结果。
///
/// `networking_enabled` 为假或 `platform` 缺失时直接返回对应的失败结果，
/// 不发起任何外部请求；调用失败同样以结果形式返回，`call_id` 指向失败审计行。
pub fn probe(
    conn: &Connection,
    networking_enabled: bool,
    platform: Option<&PlatformView>,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    key_present: bool,
) -> CoreResult<ProbeOutcome> {
    let (platform_code, model_name) = platform
        .map(|view| (view.code.clone(), view.model_name.clone()))
        .unwrap_or_default();

    if !networking_enabled {
        return Ok(blocked(platform_code, model_name, "E_NETWORK_OFF", key_present));
    }
    if platform.is_none() {
        return Ok(blocked(platform_code, model_name, "E_NOT_FOUND", key_present));
    }

    let request = ModelRequest::new(PROBE_PURPOSE, "你是连通性自检端点，只做最小确认。", PROBE_PROMPT)
        .with_prompt_version(PROBE_PROMPT_VERSION);
    let started = Instant::now();
    match call_model(conn, client, &request, policy) {
        Ok(response) => Ok(ProbeOutcome {
            ok: true,
            exit_code: EXIT_OK,
            platform_code: response.platform.clone(),
            model_name: response.model.clone(),
            latency_ms: started.elapsed().as_millis() as i64,
            call_id: last_call_id(conn, PROBE_PURPOSE)?.unwrap_or_default(),
            snippet: response.content.trim().to_string(),
            error_code: None,
            key_present,
        }),
        Err(error) => {
            let error_code = error.code().to_string();
            let mut outcome = blocked(platform_code, model_name, &error_code, key_present);
            outcome.latency_ms = started.elapsed().as_millis() as i64;
            outcome.call_id = last_call_id(conn, PROBE_PURPOSE)?.unwrap_or_default();
            Ok(outcome)
        }
    }
}
