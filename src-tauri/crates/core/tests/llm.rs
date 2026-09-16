//! 模型调用层：请求组装、响应解析、流式增量、重试与审计。

use std::cell::Cell;

use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::openai::{self, HttpTransport};
use thought_forge_core::llm::platform::{self, PlatformInput};
use thought_forge_core::llm::{call_model, GatedClient, ModelClient, ModelRequest, RetryPolicy};
use thought_forge_core::{CoreError, CoreResult};

fn memory_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().unwrap();
    migrations::apply_all(&mut conn).unwrap();
    conn
}

#[test]
fn payload_carries_model_and_messages() {
    let request = ModelRequest::new("council_round1", "系统提示", "用户问题");
    let body = openai::build_payload(&request, "gpt-x", false);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["model"], "gpt-x");
    assert_eq!(value["messages"][0]["role"], "system");
    assert_eq!(value["messages"][1]["content"], "用户问题");
    assert_eq!(value["stream"], false);
}

#[test]
fn completion_parsing_reads_content_and_usage() {
    let raw = r#"{"model":"gpt-x","choices":[{"message":{"content":"结论"}}],
        "usage":{"prompt_tokens":7,"completion_tokens":9}}"#;
    let response = openai::parse_completion(raw).unwrap();
    assert_eq!(response.content, "结论");
    assert_eq!(response.model, "gpt-x");
    assert_eq!(response.prompt_tokens, 7);
    assert_eq!(response.completion_tokens, 9);
}

#[test]
fn error_response_becomes_model_unavailable() {
    let raw = r#"{"error":{"code":429,"message":"rate limited"}}"#;
    let error = openai::parse_completion(raw).unwrap_err();
    assert_eq!(error.code(), "E_MODEL_UNAVAILABLE");
    assert!(error.to_string().contains("rate limited"));
}

#[test]
fn missing_content_is_a_malformed_response() {
    let raw = r#"{"choices":[{"message":{}}]}"#;
    let error = openai::parse_completion(raw).unwrap_err();
    assert_eq!(error.code(), "E_MALFORMED_RESPONSE");
}

#[test]
fn sse_delta_extraction() {
    let line = r#"data: {"choices":[{"delta":{"content":"你好"}}]}"#;
    assert_eq!(openai::parse_sse_delta(line), Some("你好".to_string()));
    assert_eq!(openai::parse_sse_delta("data: [DONE]"), None);
    assert_eq!(openai::parse_sse_delta(": keep-alive"), None);
}

#[test]
fn endpoint_must_be_absolute_http() {
    assert!(openai::validate_endpoint("https://api.example.com/v1/chat/completions").is_ok());
    assert!(openai::validate_endpoint("api.example.com").is_err());
}

/// 前 `fail_times` 次失败，之后成功。
struct FlakyClient {
    fail_times: Cell<u32>,
}

impl ModelClient for FlakyClient {
    fn complete(&self, request: &ModelRequest) -> CoreResult<thought_forge_core::llm::ModelResponse> {
        let remaining = self.fail_times.get();
        if remaining > 0 {
            self.fail_times.set(remaining - 1);
            return Err(CoreError::ModelUnavailable {
                status: 503,
                message: "临时故障".to_string(),
            });
        }
        Ok(thought_forge_core::llm::ModelResponse {
            content: format!("回复：{}", request.purpose),
            platform: "flaky".to_string(),
            model: "flaky-1".to_string(),
            prompt_tokens: 1,
            completion_tokens: 1,
        })
    }
}

#[test]
fn retry_recovers_and_audits_every_attempt() {
    let conn = memory_db();
    let client = FlakyClient {
        fail_times: Cell::new(2),
    };
    let policy = RetryPolicy {
        attempts: 3,
        base_delay_ms: 0,
    };
    let request = ModelRequest::new("council_round1", "sys", "usr");
    let response = call_model(&conn, &client, &request, &policy).unwrap();
    assert_eq!(response.content, "回复：council_round1");

    let calls = thought_forge_core::llm::recent_calls(&conn, 10).unwrap();
    assert_eq!(calls.len(), 3);
    let attempts: Vec<i64> = calls.iter().map(|call| call.attempt).collect();
    assert_eq!(attempts, vec![3, 2, 1], "按时间倒序返回三次尝试");
    assert_eq!(calls[0].status, "ok");
    assert_eq!(calls[1].status, "failed");
}

#[test]
fn retry_gives_up_after_the_policy_limit() {
    let conn = memory_db();
    let client = FlakyClient {
        fail_times: Cell::new(9),
    };
    let policy = RetryPolicy {
        attempts: 3,
        base_delay_ms: 0,
    };
    let request = ModelRequest::new("council_round1", "sys", "usr");
    let error = call_model(&conn, &client, &request, &policy).unwrap_err();
    assert_eq!(error.code(), "E_MODEL_UNAVAILABLE");
    assert_eq!(thought_forge_core::llm::recent_calls(&conn, 10).unwrap().len(), 3);
}

#[test]
fn gated_client_blocks_when_networking_is_off() {
    let inner = FlakyClient {
        fail_times: Cell::new(0),
    };
    let gated = GatedClient {
        enabled: false,
        inner: &inner,
    };
    let error = gated
        .complete(&ModelRequest::new("council_round1", "sys", "usr"))
        .unwrap_err();
    assert_eq!(error.code(), "E_NETWORK_OFF");
}

/// 内存传输：校验请求并返回固定响应。
struct StubTransport;

impl HttpTransport for StubTransport {
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        _body: &str,
    ) -> CoreResult<String> {
        assert!(url.starts_with("https://"));
        assert!(headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization")));
        Ok(r#"{"model":"stub","choices":[{"message":{"content":"来自 Stub"}}],
            "usage":{"prompt_tokens":2,"completion_tokens":3}}"#
            .to_string())
    }
}

#[test]
fn openai_client_round_trips_through_transport() {
    let client = openai::OpenAiCompatible::new(
        StubTransport,
        "https://api.example.com/v1/chat/completions",
        "secret",
        "stub-model",
        "example",
    );
    let response = client
        .complete(&ModelRequest::new("council_round1", "sys", "usr"))
        .unwrap();
    assert_eq!(response.content, "来自 Stub");
    assert_eq!(response.platform, "example");
    assert_eq!(response.model, "stub");
}

fn platform_input(code: &str, endpoint: &str, model: &str) -> PlatformInput {
    PlatformInput {
        code: code.to_string(),
        display_name: format!("平台 {code}"),
        endpoint: endpoint.to_string(),
        model_name: model.to_string(),
        input_price_micros_per_1k: 0,
        output_price_micros_per_1k: 0,
        currency: "CNY".to_string(),
    }
}

#[test]
fn platform_upsert_is_idempotent_and_lists_in_order() {
    let conn = memory_db();
    let first = platform::upsert(
        &conn,
        &platform_input("local", "http://127.0.0.1:11434/v1/chat/completions", "qwen"),
    )
    .unwrap();
    assert!(!first.enabled, "新平台默认不开启联网能力");
    assert_eq!(first.status, "disabled");

    let second = platform::upsert(&conn, &platform_input("cloud", "https://api.example.com/v1/chat/completions", "gpt-x"))
        .unwrap();
    assert_ne!(first.id, second.id);

    let again = platform::upsert(
        &conn,
        &platform_input("local", "http://127.0.0.1:11434/v1/chat/completions", "qwen2"),
    )
    .unwrap();
    assert_eq!(again.id, first.id, "同 code 覆盖应保留原记录");
    assert_eq!(again.created_at, first.created_at);
    assert_eq!(again.model_name, "qwen2");

    let listed = platform::list(&conn).unwrap();
    assert_eq!(listed.len(), 2, "覆盖不应新增记录");
    assert_eq!(listed[0].code, "local");
    assert_eq!(listed[1].code, "cloud");
}

#[test]
fn platform_rejects_relative_endpoint_and_enables_only_when_complete() {
    let conn = memory_db();
    let error = platform::upsert(&conn, &platform_input("bad", "not-a-url", "m")).unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");

    platform::upsert(&conn, &platform_input("blank", "", "")).unwrap();
    let error = platform::set_enabled(&conn, "blank", true).unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT", "配置不完整时不允许启用");

    platform::upsert(
        &conn,
        &platform_input("local", "http://127.0.0.1:11434/v1/chat/completions", "qwen"),
    )
    .unwrap();
    let enabled = platform::set_enabled(&conn, "local", true).unwrap();
    assert!(enabled.enabled);
    assert_eq!(enabled.status, "ready");

    let picked = platform::enabled(&conn).unwrap().expect("应有已启用平台");
    assert_eq!(picked.code, "local");

    let disabled = platform::set_enabled(&conn, "local", false).unwrap();
    assert_eq!(disabled.status, "disabled");
    assert!(platform::enabled(&conn).unwrap().is_none());
}

#[test]
fn missing_platform_reports_not_found() {
    let conn = memory_db();
    let error = platform::get(&conn, "nope").unwrap_err();
    assert_eq!(error.code(), "E_NOT_FOUND");
}
