//! 模型链路自检探针：退出码映射、未配置时不发调用、成功与失败都留审计。

use std::cell::{Cell, RefCell};

use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::platform::{self, PlatformInput};
use thought_forge_core::llm::probe::{
    self, ProbeOutcome, EXIT_MODEL_UNAVAILABLE, EXIT_NETWORK_OFF, EXIT_OK, EXIT_PLATFORM_MISSING,
    PROBE_PURPOSE,
};
use thought_forge_core::llm::{ModelClient, ModelRequest, ModelResponse, RetryPolicy};
use thought_forge_core::{CoreError, CoreResult};

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn policy() -> RetryPolicy {
    RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    }
}

fn config_platform(conn: &rusqlite::Connection) {
    platform::upsert(
        conn,
        &PlatformInput {
            code: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            endpoint: "https://api.example.com/v1/chat/completions".to_string(),
            model_name: "gpt-x".to_string(),
            input_price_micros_per_1k: 0,
            output_price_micros_per_1k: 0,
            currency: "CNY".to_string(),
        },
    )
    .expect("可写入平台");
    platform::set_enabled(conn, "openai", true).expect("可启用平台");
}

/// 按脚本返回结果的客户端，并记录调用次数。
struct ScriptedClient {
    content: RefCell<String>,
    calls: Cell<u32>,
    fail: Cell<bool>,
}

impl ScriptedClient {
    fn new(content: impl Into<String>) -> Self {
        Self {
            content: RefCell::new(content.into()),
            calls: Cell::new(0),
            fail: Cell::new(false),
        }
    }

    fn failing() -> Self {
        let client = Self::new("");
        client.fail.set(true);
        client
    }

    fn calls(&self) -> u32 {
        self.calls.get()
    }
}

impl ModelClient for ScriptedClient {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse> {
        assert_eq!(request.purpose, PROBE_PURPOSE, "探针用途应写入审计");
        self.calls.set(self.calls.get() + 1);
        if self.fail.get() {
            return Err(CoreError::ModelUnavailable {
                status: 503,
                message: "平台暂时不可用".to_string(),
            });
        }
        Ok(ModelResponse {
            content: self.content.borrow().clone(),
            platform: "openai".to_string(),
            model: "gpt-x".to_string(),
            prompt_tokens: 5,
            completion_tokens: 2,
        })
    }
}

fn audits(conn: &rusqlite::Connection) -> Vec<(String, String, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT purpose, status, COALESCE(error_code, '') FROM llm_calls
             WHERE purpose = ?1 ORDER BY rowid ASC",
        )
        .unwrap();
    let rows = stmt
        .query_map([PROBE_PURPOSE], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    rows.map(|row| row.unwrap()).collect()
}

#[test]
fn probe_succeeds_and_records_audit() {
    let conn = db();
    config_platform(&conn);
    let client = ScriptedClient::new("可用");
    let view = platform::get(&conn, "openai").unwrap();

    let outcome: ProbeOutcome =
        probe::probe(&conn, true, Some(&view), &client, &policy(), true).expect("探针可执行");

    assert!(outcome.ok);
    assert_eq!(outcome.exit_code, EXIT_OK);
    assert_eq!(outcome.platform_code, "openai");
    assert_eq!(outcome.model_name, "gpt-x");
    assert_eq!(outcome.snippet, "可用");
    assert!(outcome.error_code.is_none());
    assert!(outcome.key_present);
    assert!(!outcome.call_id.is_empty(), "应回填调用审计标识");
    assert_eq!(client.calls(), 1);

    let rows = audits(&conn);
    assert_eq!(rows.len(), 1, "成功也要留审计");
    assert_eq!(rows[0].1, "ok");
}

#[test]
fn probe_reports_network_off_without_calling() {
    let conn = db();
    config_platform(&conn);
    let client = ScriptedClient::new("可用");
    let view = platform::get(&conn, "openai").unwrap();

    let outcome = probe::probe(&conn, false, Some(&view), &client, &policy(), true).unwrap();
    assert!(!outcome.ok);
    assert_eq!(outcome.exit_code, EXIT_NETWORK_OFF);
    assert_eq!(outcome.error_code.as_deref(), Some("E_NETWORK_OFF"));
    assert_eq!(client.calls(), 0, "联网关闭时不应发起调用");
    assert!(audits(&conn).is_empty(), "未发起调用就不该有审计");
}

#[test]
fn probe_reports_missing_platform_without_calling() {
    let conn = db();
    let client = ScriptedClient::new("可用");

    let outcome = probe::probe(&conn, true, None, &client, &policy(), false).unwrap();
    assert!(!outcome.ok);
    assert_eq!(outcome.exit_code, EXIT_PLATFORM_MISSING);
    assert_eq!(outcome.error_code.as_deref(), Some("E_NOT_FOUND"));
    assert_eq!(client.calls(), 0, "平台缺失时不应发起调用");
    assert!(!outcome.key_present);
}

#[test]
fn probe_maps_model_failure_to_exit_code_and_keeps_audit() {
    let conn = db();
    config_platform(&conn);
    let client = ScriptedClient::failing();
    let view = platform::get(&conn, "openai").unwrap();

    let outcome = probe::probe(&conn, true, Some(&view), &client, &policy(), true).unwrap();
    assert!(!outcome.ok);
    assert_eq!(outcome.exit_code, EXIT_MODEL_UNAVAILABLE);
    assert_eq!(outcome.error_code.as_deref(), Some("E_MODEL_UNAVAILABLE"));
    assert!(outcome.snippet.is_empty());
    assert_eq!(client.calls(), 1);

    let rows = audits(&conn);
    assert_eq!(rows.len(), 1, "失败也要留审计");
    assert_eq!(rows[0].1, "failed");
    assert_eq!(rows[0].2, "E_MODEL_UNAVAILABLE");
    assert!(!outcome.call_id.is_empty(), "失败审计行也要回填标识");
}
