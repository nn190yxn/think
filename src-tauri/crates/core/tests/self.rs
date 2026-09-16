//! P8 自我蒸馏：解锁门槛、初稿生成、逐条确认、安装为「你」与会诊席位开关。

use std::cell::RefCell;

use proptest::prelude::*;
use serde_json::json;
use tempfile::TempDir;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::{ModelClient, ModelRequest, ModelResponse, RetryPolicy};
use thought_forge_core::master::repo as master_repo;
use thought_forge_core::self_distill::service;
use thought_forge_core::self_distill::{SELF_MASTER_ID, UNLOCK_RECORD_COUNT};
use thought_forge_core::CoreResult;

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

/// 按用途返回脚本内容的模型客户端，并记录调用次数。
struct ScriptedClient {
    content: RefCell<String>,
    calls: RefCell<u32>,
    fail: RefCell<bool>,
}

impl ScriptedClient {
    fn new(content: impl Into<String>) -> Self {
        Self {
            content: RefCell::new(content.into()),
            calls: RefCell::new(0),
            fail: RefCell::new(false),
        }
    }

    fn failing() -> Self {
        let client = Self::new("[]");
        *client.fail.borrow_mut() = true;
        client
    }

    fn calls(&self) -> u32 {
        *self.calls.borrow()
    }
}

impl ModelClient for ScriptedClient {
    fn complete(&self, _request: &ModelRequest) -> CoreResult<ModelResponse> {
        *self.calls.borrow_mut() += 1;
        if *self.fail.borrow() {
            return Err(thought_forge_core::CoreError::ModelUnavailable {
                status: 0,
                message: "脚本化失败".to_string(),
            });
        }
        Ok(ModelResponse {
            content: self.content.borrow().clone(),
            platform: "scripted".to_string(),
            model: "scripted".to_string(),
            prompt_tokens: 1,
            completion_tokens: 1,
        })
    }
}

fn seed_records(conn: &rusqlite::Connection, count: usize) {
    let tx = conn.unchecked_transaction().unwrap();
    for index in 0..count {
        tx.execute(
            "INSERT INTO thought_records
                 (id, session_id, question, topic_key, domains_json, layers_json,
                  conclusion, adopted, reason, created_at)
             VALUES (?1, NULL, ?2, 'topic', '[]', '[]', ?3, ?4, '', ?5)",
            rusqlite::params![
                format!("record-{index:03}"),
                format!("议题 {index}"),
                format!("结论 {index}"),
                i64::from(index % 2 == 0),
                format!("2026-09-{:02}T10:00:00Z", (index % 28) + 1),
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

fn candidates_json(count: usize) -> String {
    let layers = ["dao", "fa", "shu", "qi", "tool", "shi"];
    let items: Vec<serde_json::Value> = (0..count)
        .map(|index| {
            json!({
                "title": format!("我的判断 {index}"),
                "layer": layers[index % layers.len()],
                "triggerCondition": "遇到同类议题时",
                "steps": ["先看前提", "再下判断"],
                "mechanism": "沿用我过去的取舍习惯",
                "boundary": "信息充分时更可靠",
                "records": [index % 5],
            })
        })
        .collect();
    serde_json::to_string(&items).unwrap()
}

fn setup_records(conn: &rusqlite::Connection) {
    seed_records(conn, UNLOCK_RECORD_COUNT as usize);
}

#[test]
fn readiness_locks_before_enough_records() {
    let mut conn = db();
    seed_records(&conn, 19);
    let readiness = service::readiness(&conn).unwrap();
    assert!(!readiness.unlocked);
    assert!(!readiness.installed);
    assert_eq!(readiness.required, UNLOCK_RECORD_COUNT);

    let client = ScriptedClient::new(candidates_json(3));
    let error = service::start(&mut conn, &client, &policy()).unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
    assert_eq!(client.calls(), 0, "没解锁时不应发起模型调用");
}

#[test]
fn start_generates_pending_items() {
    let mut conn = db();
    setup_records(&conn);
    let client = ScriptedClient::new(candidates_json(4));
    let detail = service::start(&mut conn, &client, &policy()).unwrap();

    assert_eq!(detail.draft.status, "ready");
    assert_eq!(detail.draft.record_count, UNLOCK_RECORD_COUNT);
    assert_eq!(detail.items.len(), 4);
    assert_eq!(detail.pending_count, 4);
    assert_eq!(detail.accepted_count, 0);
    assert!(detail.items.iter().all(|item| item.status == "pending"));
    assert!(detail.items.iter().all(|item| !item.evidence.is_empty()));
    assert_eq!(detail.draft.model_calls, 1);
}

#[test]
fn malformed_response_marks_draft_failed() {
    let mut conn = db();
    setup_records(&conn);
    let client = ScriptedClient::new("这不是 JSON");
    let error = service::start(&mut conn, &client, &policy()).unwrap_err();
    assert_eq!(error.code(), "E_MALFORMED_RESPONSE");

    let readiness = service::readiness(&conn).unwrap();
    let draft = readiness.latest_draft.expect("失败草稿应保留");
    assert_eq!(draft.status, "failed");
    assert_eq!(draft.error_code.as_deref(), Some("E_MALFORMED_RESPONSE"));
}

#[test]
fn model_failure_marks_draft_failed() {
    let mut conn = db();
    setup_records(&conn);
    let client = ScriptedClient::failing();
    let error = service::start(&mut conn, &client, &policy()).unwrap_err();
    assert_eq!(error.code(), "E_MODEL_UNAVAILABLE");
    assert_eq!(
        service::readiness(&conn).unwrap().latest_draft.unwrap().status,
        "failed"
    );
}

#[test]
fn pending_items_block_install() {
    let mut conn = db();
    setup_records(&conn);
    let client = ScriptedClient::new(candidates_json(3));
    let detail = service::start(&mut conn, &client, &policy()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let error = service::install(&mut conn, &detail.draft.id, dir.path()).unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
}

#[test]
fn install_requires_adopted_item() {
    let mut conn = db();
    setup_records(&conn);
    let client = ScriptedClient::new(candidates_json(3));
    let detail = service::start(&mut conn, &client, &policy()).unwrap();
    for item in &detail.items {
        service::decide(&conn, &detail.draft.id, &item.id, false).unwrap();
    }

    let dir = tempfile::tempdir().unwrap();
    let error = service::install(&mut conn, &detail.draft.id, dir.path()).unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
}

#[test]
fn install_creates_self_master_and_enables_seat() {
    let mut conn = db();
    setup_records(&conn);
    let client = ScriptedClient::new(candidates_json(3));
    let detail = service::start(&mut conn, &client, &policy()).unwrap();
    service::decide(&conn, &detail.draft.id, &detail.items[0].id, true).unwrap();
    service::decide(&conn, &detail.draft.id, &detail.items[1].id, false).unwrap();
    service::decide(&conn, &detail.draft.id, &detail.items[2].id, true).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let outcome = service::install(&mut conn, &detail.draft.id, dir.path()).unwrap();
    assert_eq!(outcome.master_id, SELF_MASTER_ID);
    assert_eq!(outcome.version, 1);
    assert_eq!(outcome.unit_count, 2);
    assert!(outcome.created);

    let master = master_repo::detail(&conn, SELF_MASTER_ID).expect("你已安装");
    assert_eq!(master.units.len(), 2);

    let readiness = service::readiness(&conn).unwrap();
    assert!(readiness.installed);
    assert!(readiness.seat_enabled);
    assert_eq!(readiness.current_version, 1);
    assert_eq!(
        service::seat_master_id(&conn).unwrap().as_deref(),
        Some(SELF_MASTER_ID)
    );

    // 安装后草稿不可再改。
    let error = service::decide(&conn, &detail.draft.id, &detail.items[0].id, true).unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
}

#[test]
fn reinstall_increments_version() {
    let mut conn = db();
    setup_records(&conn);
    let dir = tempfile::tempdir().unwrap();

    for expected in 1..=2 {
        let client = ScriptedClient::new(candidates_json(2));
        let detail = service::start(&mut conn, &client, &policy()).unwrap();
        for item in &detail.items {
            service::decide(&conn, &detail.draft.id, &item.id, true).unwrap();
        }
        let outcome = service::install(&mut conn, &detail.draft.id, dir.path()).unwrap();
        assert_eq!(outcome.version, expected);
    }
    assert_eq!(service::readiness(&conn).unwrap().current_version, 2);
}

#[test]
fn seat_can_be_disabled() {
    let mut conn = db();
    setup_records(&conn);
    let client = ScriptedClient::new(candidates_json(2));
    let detail = service::start(&mut conn, &client, &policy()).unwrap();
    for item in &detail.items {
        service::decide(&conn, &detail.draft.id, &item.id, true).unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    service::install(&mut conn, &detail.draft.id, dir.path()).unwrap();

    let readiness = service::set_seat_enabled(&conn, false).unwrap();
    assert!(!readiness.seat_enabled);
    assert!(service::seat_master_id(&conn).unwrap().is_none());

    // 关闭后重新开启仍然有席位。
    assert!(service::set_seat_enabled(&conn, true).unwrap().seat_enabled);
}

#[test]
fn revoked_principle_disappears_from_self_realm() {
    use thought_forge_core::companion::growth;
    use thought_forge_core::council::echo;
    use thought_forge_core::master::Layer;
    use thought_forge_core::network::NodeKind;
    use thought_forge_core::network::repo::{self as network_repo, NewNode};

    let conn = db();
    let node_id = network_repo::upsert_node(
        &conn,
        &NewNode {
            kind: NodeKind::Principle,
            content: "凡事先小步验证再放大",
            source_kind: growth::SOURCE_PRINCIPLE,
            source_ref: "topic:验证",
            domains: &["自我".to_string()],
            layers: &[Layer::Dao],
        },
    )
    .expect("可写入原则")
    .node_id;

    let before = growth::list_principles(&conn, 10).unwrap();
    assert!(before.iter().any(|seal| seal.node_id == node_id));
    let count_before = growth::ring_overview(&conn).unwrap().principle_count;

    echo::revoke_principle(&conn, &node_id, "已被新证据推翻").expect("可撤销原则");

    let after = growth::list_principles(&conn, 10).unwrap();
    assert!(
        after.iter().all(|seal| seal.node_id != node_id),
        "撤销后我境界不再陈列"
    );
    assert_eq!(
        growth::ring_overview(&conn).unwrap().principle_count,
        count_before - 1,
        "年轮原则数应随之减少"
    );
    assert_eq!(
        echo::revoked_principles(&conn, 10).unwrap().len(),
        1,
        "撤销留痕仍可读"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// 安装成功当且仅当至少采纳一条框架，且没有待确认条目。
    #[test]
    fn property_install_requires_adopted_item(adopted in prop::collection::vec(any::<bool>(), 1..6)) {
        let mut conn = db();
        setup_records(&conn);
        let client = ScriptedClient::new(candidates_json(adopted.len()));
        let detail = service::start(&mut conn, &client, &policy()).unwrap();
        prop_assert_eq!(detail.items.len(), adopted.len());
        for (item, decision) in detail.items.iter().zip(adopted.iter()) {
            service::decide(&conn, &detail.draft.id, &item.id, *decision).unwrap();
        }
        let dir = TempDir::new().unwrap();
        let outcome = service::install(&mut conn, &detail.draft.id, dir.path());
        if adopted.iter().any(|value| *value) {
            prop_assert!(outcome.is_ok(), "有采纳条目时应能安装");
        } else {
            prop_assert!(outcome.is_err(), "全部剔除时不应安装");
        }
    }
}
