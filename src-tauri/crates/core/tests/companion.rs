//! 主动助理：推送上限、开关生效、三类洞察、原则提升与年轮概览。

use std::cell::Cell;

use proptest::prelude::*;
use thought_forge_core::companion::collide::{self as collide_mod};
use thought_forge_core::companion::{
    growth, repo, CollisionSignal, CompanionRules, InsightFilter, InsightKind, NewInsight,
    SOURCE_COMPANION,
};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::{ModelClient, ModelRequest, ModelResponse, RetryPolicy};
use thought_forge_core::master::Layer;
use thought_forge_core::network::recorder::SOURCE_RECORD;
use thought_forge_core::network::repo as network_repo;
use thought_forge_core::network::NodeKind;
use thought_forge_core::CoreResult;

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

/// 返回固定内容的模型客户端，并记录被调用次数。
struct ScriptedClient {
    content: String,
    calls: Cell<u32>,
}

impl ScriptedClient {
    fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            calls: Cell::new(0),
        }
    }
}

impl ModelClient for ScriptedClient {
    fn complete(&self, _request: &ModelRequest) -> CoreResult<ModelResponse> {
        self.calls.set(self.calls.get() + 1);
        Ok(ModelResponse {
            content: self.content.clone(),
            platform: "stub".to_string(),
            model: "stub".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
        })
    }
}

fn policy() -> RetryPolicy {
    RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    }
}

fn signal(content: &str) -> CollisionSignal {
    CollisionSignal {
        source_kind: "thought_record".to_string(),
        source_ref: "record-1".to_string(),
        content: content.to_string(),
        domains: Vec::new(),
        layers: Vec::new(),
    }
}

fn json_insights(count: usize) -> String {
    let items: Vec<String> = (0..count)
        .map(|index| {
            format!(
                "{{\"kind\":\"relation\",\"title\":\"关联{index}\",\"summary\":\"说明{index}\",\"nodeIds\":[],\"masterIds\":[]}}"
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn add_node(conn: &rusqlite::Connection, content: &str) -> String {
    network_repo::upsert_node(
        conn,
        &network_repo::NewNode {
            kind: NodeKind::Idea,
            content,
            source_kind: "test",
            source_ref: "test",
            domains: &[],
            layers: &[],
        },
    )
    .unwrap()
    .node_id
}

fn companion_insight_count(conn: &rusqlite::Connection) -> usize {
    repo::list_insights(
        conn,
        &InsightFilter {
            source: Some(SOURCE_COMPANION.to_string()),
            ..InsightFilter::default()
        },
        200,
    )
    .unwrap()
    .len()
}

#[test]
fn settings_default_to_disabled_and_persist() {
    let conn = db();
    let settings = repo::get_settings(&conn).unwrap();
    assert!(!settings.enabled, "主动助学默认关闭");
    assert_eq!(settings.daily_limit, 5);
    assert!(settings.rules.trigger_on_record);

    let enabled = repo::set_enabled(&conn, true).unwrap();
    assert!(enabled.enabled);
    assert!(repo::get_settings(&conn).unwrap().enabled);

    let limited = repo::set_daily_limit(&conn, 8).unwrap();
    assert_eq!(limited.daily_limit, 8);
    assert!(repo::set_daily_limit(&conn, 0).is_err(), "下限为 1");

    let rules = CompanionRules {
        trigger_on_capture: false,
        context_nodes: 3,
        ..CompanionRules::default()
    };
    let saved = repo::set_rules(&conn, &rules).unwrap();
    assert!(!saved.rules.trigger_on_capture);
    assert_eq!(saved.rules.context_nodes, 3);
}

#[test]
fn disabled_companion_produces_no_insights() {
    let conn = db();
    add_node(&conn, "注意力是稀缺资源");
    let client = ScriptedClient::new(json_insights(3));

    let outcome = collide_mod::collide(&conn, &client, &signal("注意力该如何分配"), &policy()).unwrap();
    assert_eq!(outcome.skipped.as_deref(), Some("disabled"));
    assert!(outcome.generated.is_empty());
    assert_eq!(client.calls.get(), 0, "关闭时不应发起模型调用");
    assert_eq!(companion_insight_count(&conn), 0);
}

#[test]
fn collide_stops_at_daily_limit() {
    let conn = db();
    add_node(&conn, "注意力是稀缺资源");
    repo::set_enabled(&conn, true).unwrap();
    repo::set_daily_limit(&conn, 2).unwrap();
    let client = ScriptedClient::new(json_insights(4));

    let first = collide_mod::collide(&conn, &client, &signal("注意力该如何分配"), &policy()).unwrap();
    assert_eq!(first.pushed, 2, "当日上限为 2");
    assert_eq!(first.remaining, 0);
    assert_eq!(companion_insight_count(&conn), 2);

    let second = collide_mod::collide(&conn, &client, &signal("注意力该如何分配"), &policy()).unwrap();
    assert_eq!(second.skipped.as_deref(), Some("limit"));
    assert!(second.generated.is_empty());
    assert_eq!(companion_insight_count(&conn), 2, "超限后不再写入");
}

#[test]
fn collide_generates_blindspot_when_graph_has_no_match() {
    let conn = db();
    repo::set_enabled(&conn, true).unwrap();
    let client = ScriptedClient::new("[]");

    let outcome = collide_mod::collide(&conn, &client, &signal("从未想过的一个新领域"), &policy()).unwrap();
    assert_eq!(outcome.pushed, 1);
    assert_eq!(outcome.generated[0].kind, InsightKind::Blindspot);
    assert_eq!(client.calls.get(), 1);
}

#[test]
fn capture_rule_can_disable_trigger() {
    let conn = db();
    repo::set_enabled(&conn, true).unwrap();
    repo::set_rules(
        &conn,
        &CompanionRules {
            trigger_on_capture: false,
            ..CompanionRules::default()
        },
    )
    .unwrap();
    let client = ScriptedClient::new(json_insights(2));

    let mut capture = signal("剪贴板里的新内容");
    capture.source_kind = "capture".to_string();
    let outcome = collide_mod::collide(&conn, &client, &capture, &policy()).unwrap();
    assert_eq!(outcome.skipped.as_deref(), Some("rule_capture_off"));
    assert_eq!(client.calls.get(), 0);
}

#[test]
fn parse_insights_tolerates_code_fences() {
    let raw = "这里是结果：\n```json\n[{\"kind\":\"conflict\",\"title\":\"冲突点\",\"summary\":\"说明\",\"nodeIds\":[\"n1\"],\"masterIds\":[]}]\n```\n以上。";
    let parsed = collide_mod::parse_insights(raw);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].kind, "conflict");
    assert_eq!(parsed[0].node_ids, vec!["n1".to_string()]);
}

#[test]
fn mark_insight_records_disposition() {
    let conn = db();
    let insight = repo::insert_insight(
        &conn,
        &NewInsight {
            kind: InsightKind::Relation,
            title: "两个判断其实同源",
            summary: "都指向长期主义",
            related_node_ids: &[],
            related_master_ids: &[],
            evidence: &[],
            source: SOURCE_COMPANION,
        },
    )
    .unwrap();
    assert_eq!(insight.status, "new");

    let adopted = repo::mark_insight(&conn, &insight.id, "adopt", "已按此调整").unwrap();
    assert_eq!(adopted.status, "adopted");
    assert_eq!(adopted.action, "adopt");
    assert_eq!(adopted.reason, "已按此调整");

    let converted = repo::mark_insight(&conn, &insight.id, "convert", "").unwrap();
    assert_eq!(converted.status, "converted");
    assert!(repo::mark_insight(&conn, &insight.id, "unknown", "").is_err());
}

#[test]
fn promote_principles_after_three_adoptions() {
    let conn = db();
    let question = "要不要把稳定的工作换成独立做产品";
    let mut last_judgment = String::new();
    for index in 0..3 {
        let (record_id, topic_key) = network_repo::write_record(
            &conn,
            &network_repo::NewRecord {
                session_id: None,
                question,
                domains: &["职业".to_string()],
                layers: &[Layer::Dao],
                conclusion: &format!("先收敛现金流再看节奏（第{index}版）"),
            },
        )
        .unwrap();
        assert_eq!(topic_key, "要不要把稳定的工作换成独立做产品");
        let node = network_repo::upsert_node(
            &conn,
            &network_repo::NewNode {
                kind: NodeKind::Judgment,
                content: &format!("先收敛现金流再看节奏（第{index}版）"),
                source_kind: SOURCE_RECORD,
                source_ref: &record_id,
                domains: &["职业".to_string()],
                layers: &[Layer::Dao],
            },
        )
        .unwrap();
        network_repo::mark_record_decision(&conn, &record_id, true, "已执行").unwrap();
        last_judgment = node.node_id;
    }

    let promoted = growth::promote_principles(&conn).unwrap();
    assert_eq!(promoted.len(), 1, "连续采纳三次后提升为原则");
    let principle_id = promoted[0].clone();

    let detail = network_repo::get_node(&conn, &last_judgment).unwrap();
    assert!(
        detail
            .links
            .iter()
            .any(|link| link.peer_id == principle_id),
        "原判断节点保留并指向原则节点"
    );

    assert!(
        growth::promote_principles(&conn).unwrap().is_empty(),
        "同一主题只提升一次"
    );

    let seals = growth::list_principles(&conn, 10).unwrap();
    assert_eq!(seals.len(), 1);
    assert_eq!(seals[0].adopted_count, 3);
    assert_eq!(seals[0].node_id, principle_id);
}

#[test]
fn list_topics_aggregates_records() {
    let conn = db();
    for (index, adopted) in [(0, true), (1, false)] {
        let (record_id, _) = network_repo::write_record(
            &conn,
            &network_repo::NewRecord {
                session_id: None,
                question: "如何取舍扩张与收敛",
                domains: &[],
                layers: &[],
                conclusion: &format!("结论{index}"),
            },
        )
        .unwrap();
        network_repo::mark_record_decision(&conn, &record_id, adopted, "").unwrap();
    }

    let topics = growth::list_topics(&conn, 10).unwrap();
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0].record_count, 2);
    assert_eq!(topics[0].adopted_count, 1);
}

#[test]
fn ring_overview_reports_growth_metrics() {
    let conn = db();
    add_node(&conn, "节点甲");
    add_node(&conn, "节点乙");
    let overview = growth::ring_overview(&conn).unwrap();
    assert_eq!(overview.top_nodes.len(), 2);
    assert_eq!(overview.principle_count, 0);
    assert!(overview.fastest_domains.is_empty());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// 属性 15：单个自然日内，助理推送的洞察数量不超过用户设定的上限。
    #[test]
    fn property_push_never_exceeds_daily_limit(
        limit in 1i64..=8,
        candidates in 1usize..=12,
    ) {
        let conn = db();
        add_node(&conn, "注意力是稀缺资源");
        repo::set_enabled(&conn, true).unwrap();
        repo::set_daily_limit(&conn, limit).unwrap();
        let client = ScriptedClient::new(json_insights(candidates));

        let outcome = collide_mod::collide(&conn, &client, &signal("注意力该如何分配"), &policy()).unwrap();
        prop_assert!(outcome.pushed <= limit, "一次碰撞写入超过上限");
        prop_assert!(outcome.remaining >= 0);
        prop_assert!(companion_insight_count(&conn) as i64 <= limit);

        // 再次碰撞也只会在剩余额度内写入。
        let again = collide_mod::collide(&conn, &client, &signal("注意力该如何分配"), &policy()).unwrap();
        prop_assert!(companion_insight_count(&conn) as i64 <= limit);
        prop_assert!(again.pushed <= limit);
    }

    /// 属性 17：主动助学关闭后不产生新的主动推送，也不发起模型调用。
    #[test]
    fn property_disabled_companion_never_pushes(content in ".{1,40}") {
        let conn = db();
        add_node(&conn, "注意力是稀缺资源");
        let client = ScriptedClient::new(json_insights(3));

        let outcome = collide_mod::collide(&conn, &client, &signal(&content), &policy()).unwrap();
        prop_assert_eq!(outcome.pushed, 0);
        prop_assert_eq!(outcome.skipped.as_deref(), Some("disabled"));
        prop_assert_eq!(client.calls.get(), 0);
        prop_assert_eq!(companion_insight_count(&conn), 0);
    }
}
