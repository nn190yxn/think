//! 思维网络：节点去重、关系对称、激活单调、时间衰减与固化幂等。

use proptest::prelude::*;
use thought_forge_core::council::{repo as council, Strategy};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::master::Layer;
use thought_forge_core::network::repo::{self, NewNode, NewRecord, DEFAULT_GRAPH_LIMIT};
use thought_forge_core::network::{consolidate, recorder, GraphFilter, NodeKind, Relation};

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

/// 建节点的小助手：返回 (连接, 节点 id)。
fn add(conn: &rusqlite::Connection, kind: NodeKind, content: &str, source: &str) -> String {
    repo::upsert_node(
        conn,
        &NewNode {
            kind,
            content,
            source_kind: source,
            source_ref: source,
            domains: &[],
            layers: &[],
        },
    )
    .expect("节点可写入")
    .node_id
}

#[test]
fn upsert_node_merges_same_normalized_content() {
    let conn = db();
    let first = repo::upsert_node(
        &conn,
        &NewNode {
            kind: NodeKind::Idea,
            content: "先看动机，再谈取舍。",
            source_kind: "capture",
            source_ref: "a",
            domains: &["growth".to_string()],
            layers: &[Layer::Dao],
        },
    )
    .unwrap();
    let second = repo::upsert_node(
        &conn,
        &NewNode {
            kind: NodeKind::Idea,
            content: "  先看动机再谈取舍 ",
            source_kind: "capture",
            source_ref: "b",
            domains: &["self".to_string()],
            layers: &[Layer::Qi],
        },
    )
    .unwrap();

    assert_eq!(first.outcome, "created");
    assert_eq!(second.outcome, "matched");
    assert_eq!(first.node_id, second.node_id);

    let detail = repo::get_node(&conn, &first.node_id).unwrap();
    assert!(detail.node.domains.contains(&"growth".to_string()));
    assert!(detail.node.domains.contains(&"self".to_string()));
    assert!(detail.node.layers.contains(&Layer::Dao));
    assert!(detail.node.layers.contains(&Layer::Qi));
}

#[test]
fn conflict_edge_is_symmetric_and_deduplicated() {
    let conn = db();
    let a = add(&conn, NodeKind::Judgment, "规模优先", "test");
    let b = add(&conn, NodeKind::Judgment, "质量优先", "test");

    let forward = repo::link_nodes(&conn, &a, &b, Relation::Conflicts, 0.6).unwrap();
    let backward = repo::link_nodes(&conn, &b, &a, Relation::Conflicts, 0.8).unwrap();
    assert!(forward.created);
    assert!(!backward.created, "冲突关系双向调用应命中同一条记录");
    assert_eq!(forward.edge_id, backward.edge_id);
    assert_eq!(backward.weight, 0.8, "重复声明取较大权重");

    let forward_supports = repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.5).unwrap();
    let backward_supports = repo::link_nodes(&conn, &b, &a, Relation::Supports, 0.5).unwrap();
    assert!(forward_supports.created);
    assert!(backward_supports.created, "非对称关系保持方向语义");
    assert_ne!(forward_supports.edge_id, backward_supports.edge_id);
}

#[test]
fn activation_is_monotonic_and_propagates_one_hop() {
    let conn = db();
    let a = add(&conn, NodeKind::Idea, "注意力是稀缺资源", "test");
    let b = add(&conn, NodeKind::Idea, "把时间花在复利上", "test");
    repo::link_nodes(&conn, &a, &b, Relation::Analogous, 0.9).unwrap();

    let before = repo::get_node(&conn, &a).unwrap().node.activation;
    let first = repo::activate(&conn, std::slice::from_ref(&a), 1.0, None).unwrap();
    let after = repo::get_node(&conn, &a).unwrap().node.activation;
    assert!(after >= before, "激发后不低于激发前");
    assert_eq!(first.activated, 1);
    assert_eq!(first.propagated, 1, "邻居应被一跳传播唤醒");

    let neighbor = repo::get_node(&conn, &b).unwrap().node.activation;
    assert!(neighbor > 0.0, "邻居激活度应被提升");
    assert!(neighbor < after, "邻居增益按权重折半，应低于直接激活");

    let second = repo::activate(&conn, std::slice::from_ref(&a), 1.0, None).unwrap();
    let again = repo::get_node(&conn, &a).unwrap().node.activation;
    assert!(again >= after);
    assert_eq!(second.activated, 1);

    let detail = repo::get_node(&conn, &a).unwrap();
    assert_eq!(detail.activations.len(), 2, "每次唤醒都留痕");
}

#[test]
fn activation_propagates_multiple_hops_and_respects_settings() {
    use thought_forge_core::council::tuning;

    let conn = db();
    let a = add(&conn, NodeKind::Idea, "第一跳起点", "test");
    let b = add(&conn, NodeKind::Idea, "中间节点", "test");
    let c = add(&conn, NodeKind::Idea, "远端节点", "test");
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.8).unwrap();
    repo::link_nodes(&conn, &b, &c, Relation::Supports, 0.8).unwrap();

    let two_hops = repo::activate(&conn, std::slice::from_ref(&a), 1.0, None).unwrap();
    assert_eq!(two_hops.propagated, 1, "第一跳唤醒中间节点");
    assert_eq!(two_hops.propagated_far, 1, "第二跳唤醒远端节点");
    assert_eq!(two_hops.hops, 2);

    let middle = repo::get_node(&conn, &b).unwrap().node.activation;
    let far = repo::get_node(&conn, &c).unwrap().node.activation;
    assert!(far > 0.0 && far < middle, "第二跳增益再乘跳间衰减");

    // 跳到 1 时不再有远端传播。
    tuning::set(&conn, &[("network.hop_count".to_string(), "1".to_string())]).unwrap();
    let one_hop = repo::activate(&conn, std::slice::from_ref(&a), 1.0, None).unwrap();
    assert_eq!(one_hop.hops, 1);
    assert_eq!(one_hop.propagated_far, 0);
}

#[test]
fn activation_skips_edges_below_the_weight_floor_on_far_hops() {
    use thought_forge_core::council::tuning;

    let conn = db();
    let a = add(&conn, NodeKind::Idea, "起点", "test");
    let b = add(&conn, NodeKind::Idea, "中间", "test");
    let c = add(&conn, NodeKind::Idea, "远端", "test");
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.8).unwrap();
    repo::link_nodes(&conn, &b, &c, Relation::Supports, 0.01).unwrap();

    tuning::set(&conn, &[("network.min_edge_weight".to_string(), "0.5".to_string())]).unwrap();
    let outcome = repo::activate(&conn, std::slice::from_ref(&a), 1.0, None).unwrap();
    assert_eq!(outcome.propagated, 1);
    assert_eq!(outcome.propagated_far, 0, "低于阈值的连线不参与远端传播");
}

#[test]
fn decay_factor_halves_over_one_half_life() {
    assert!((repo::decay_factor(0.0, 168.0) - 1.0).abs() < f64::EPSILON);
    assert!((repo::decay_factor(168.0, 168.0) - 0.5).abs() < 1e-9);
    assert!((repo::decay_factor(336.0, 168.0) - 0.25).abs() < 1e-9);
    assert!((repo::decay_factor(100.0, 0.0) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn graph_returns_nodes_and_internal_edges_only() {
    let conn = db();
    let a = add(&conn, NodeKind::Idea, "节点甲", "test");
    let b = add(&conn, NodeKind::Framework, "节点乙", "test");
    let c = add(&conn, NodeKind::Question, "节点丙", "test");
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.7).unwrap();
    repo::link_nodes(&conn, &b, &c, Relation::Derives, 0.4).unwrap();

    let view = repo::get_graph(&conn, &GraphFilter::default()).unwrap();
    assert_eq!(view.nodes.len(), 3);
    assert_eq!(view.edges.len(), 2);
    assert!(!view.truncated);
    assert_eq!(view.total_nodes, 3);

    let filtered = repo::get_graph(
        &conn,
        &GraphFilter {
            kind: Some(NodeKind::Idea),
            ..GraphFilter::default()
        },
    )
    .unwrap();
    assert_eq!(filtered.nodes.len(), 1);
    assert!(filtered.edges.is_empty(), "另一端点不在集合内的连线不返回");
    assert!(filtered.truncated);
}

#[test]
fn resolve_conflict_records_decision() {
    let conn = db();
    let a = add(&conn, NodeKind::Judgment, "应当扩张", "test");
    let b = add(&conn, NodeKind::Judgment, "应当收敛", "test");
    let edge = repo::link_nodes(&conn, &a, &b, Relation::Conflicts, 0.6).unwrap();

    repo::resolve_conflict(&conn, &edge.edge_id, "keep", "两者在不同阶段都成立").unwrap();
    let detail = repo::get_node(&conn, &a).unwrap();
    let link = detail
        .links
        .iter()
        .find(|link| link.edge_id == edge.edge_id)
        .expect("裁决后连线仍在");
    assert_eq!(link.status, "kept");
    assert_eq!(link.direction, "both");

    let insights: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM insights WHERE kind = 'conflict_decision'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(insights, 1);
}

/// 构造一次已完成会诊，不调用模型。
fn finished_council(conn: &rusqlite::Connection, question: &str, conclusion: &str) -> String {
    let session = council::create_session(
        conn,
        question,
        &["growth".to_string()],
        &[Layer::Dao, Layer::Shu],
        Strategy::Steady,
    )
    .unwrap();
    for (index, answer) in ["先看长期复利，再谈短期节奏", "先守住现金流，再谈扩张速度"]
        .iter()
        .enumerate()
    {
        council::save_turn(
            conn,
            &council::NewTurn {
                session_id: session.clone(),
                round: 1,
                panel_rotation: 0,
                role: "answer".to_string(),
                master_id: Some(format!("master-{index}")),
                master_version: Some(1),
                content: (*answer).to_string(),
                citations: Vec::new(),
                prompt_version: "2026-09-15.1".to_string(),
                status: "ok".to_string(),
                error_code: None,
            },
        )
        .unwrap();
    }
    council::finish_session(conn, &session, conclusion, &["扩张与收敛的先后顺序".to_string()])
        .unwrap();
    session
}

#[test]
fn record_session_writes_network_and_is_stable_across_repeats() {
    let conn = db();
    let session = finished_council(&conn, "如何取舍扩张与收敛", "先收敛现金流，再按节奏扩张");

    let first = recorder::record_session(&conn, &session).unwrap();
    assert_eq!(first.framework_ids.len(), 2);
    assert_eq!(first.divergence_ids.len(), 1);
    assert!(first.activated >= 4, "判断、框架与分歧都应被唤起");

    let detail = repo::get_node(&conn, &first.judgment_id).unwrap();
    assert_eq!(detail.node.kind, NodeKind::Judgment);
    assert!(detail
        .links
        .iter()
        .any(|link| link.relation == Relation::Conflicts));

    let nodes_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM thought_nodes", [], |row| row.get(0))
        .unwrap();

    let second = recorder::record_session(&conn, &session).unwrap();
    assert_eq!(
        second.judgment_id, first.judgment_id,
        "同内容判断应复用既有节点"
    );
    assert_eq!(second.framework_ids, first.framework_ids);
    assert_eq!(second.divergence_ids, first.divergence_ids);
    let nodes_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM thought_nodes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(nodes_after, nodes_before, "重复写入不产生重复节点");
}

#[test]
fn repeated_council_links_judgments_into_evolution_chain() {
    let conn = db();
    let first_session = finished_council(&conn, "如何取舍扩张与收敛", "先扩张再看数据");
    let first = recorder::record_session(&conn, &first_session).unwrap();

    let second_session = finished_council(&conn, "如何取舍扩张与收敛", "先收敛再扩张");
    let second = recorder::record_session(&conn, &second_session).unwrap();
    assert_eq!(second.linked_prior, vec![first.judgment_id.clone()]);

    let detail = repo::get_node(&conn, &second.judgment_id).unwrap();
    assert!(detail
        .links
        .iter()
        .any(|link| link.peer_id == first.judgment_id && link.relation == Relation::Derives));

    let topic_rows = repo::list_records(&conn, 10).unwrap();
    assert_eq!(topic_rows.len(), 2);
    let topic_key = topic_rows[0].topic_key.clone();
    let chain = repo::compare_records(&conn, &topic_key).unwrap();
    assert_eq!(chain.len(), 2, "同一主题下两条记录构成演化链");
    assert!(chain[0].created_at <= chain[1].created_at);
}

#[test]
fn record_decision_is_marked() {
    let conn = db();
    let (record_id, _) = repo::write_record(
        &conn,
        &NewRecord {
            session_id: None,
            question: "要不要换城市",
            domains: &[],
            layers: &[],
            conclusion: "先试住三个月",
        },
    )
    .unwrap();
    repo::mark_record_decision(&conn, &record_id, true, "已按此执行").unwrap();
    let records = repo::list_records(&conn, 10).unwrap();
    assert!(records[0].adopted);
    assert_eq!(records[0].reason, "已按此执行");
}

#[test]
fn consolidation_strengthens_then_is_idempotent() {
    let conn = db();
    let a = add(&conn, NodeKind::Idea, "复利来自长期", "test");
    let b = add(&conn, NodeKind::Idea, "长期来自耐心", "test");
    let edge = repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.5).unwrap();
    repo::activate(&conn, &[a.clone(), b.clone()], 1.0, None).unwrap();

    let first = consolidate::trigger_consolidation(&conn, "manual").unwrap();
    assert!(first.strengthened_count >= 1);
    let after_first: f64 = conn
        .query_row(
            "SELECT weight FROM thought_edges WHERE id = ?1",
            [edge.edge_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(after_first > 0.5);

    let nodes_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM thought_nodes", [], |row| row.get(0))
        .unwrap();
    let insights_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM insights", [], |row| row.get(0))
        .unwrap();

    let second = consolidate::trigger_consolidation(&conn, "idle").unwrap();
    assert_eq!(second.strengthened_count, 0, "计数清零后不再重复强化");
    assert_eq!(second.merged_count, 0);
    assert_eq!(second.conflict_count, 0);
    let after_second: f64 = conn
        .query_row(
            "SELECT weight FROM thought_edges WHERE id = ?1",
            [edge.edge_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after_first, after_second, "重复固化不改变权重");
    let nodes_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM thought_nodes", [], |row| row.get(0))
        .unwrap();
    let insights_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM insights", [], |row| row.get(0))
        .unwrap();
    assert_eq!(nodes_after, nodes_before);
    assert_eq!(insights_after, insights_before);
}

#[test]
fn consolidation_merges_similar_nodes_of_same_source() {
    let conn = db();
    add(&conn, NodeKind::Idea, "注意力是稀缺资源", "capture");
    add(&conn, NodeKind::Idea, "注意力是稀缺的资源", "capture");
    // 来源不同则不合并。
    add(&conn, NodeKind::Idea, "注意力是稀缺的资源啊", "other");

    let report = consolidate::trigger_consolidation(&conn, "manual").unwrap();
    assert_eq!(report.merged_count, 1);
    assert_eq!(report.merged.len(), 1);
    let (dropped, kept) = &report.merged[0];
    let superseded: Option<String> = conn
        .query_row(
            "SELECT superseded_by FROM thought_nodes WHERE id = ?1",
            [dropped.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(superseded.as_deref(), Some(kept.as_str()));

    let graph = repo::get_graph(&conn, &GraphFilter::default()).unwrap();
    assert_eq!(graph.nodes.len(), 2, "被合并节点退出图谱");
}

#[test]
fn consolidation_detects_conflict_pair_once() {
    let conn = db();
    let a = add(&conn, NodeKind::Judgment, "应当扩张", "test");
    let b = add(&conn, NodeKind::Judgment, "应当收敛", "test");
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.5).unwrap();
    repo::link_nodes(&conn, &a, &b, Relation::Conflicts, 0.5).unwrap();

    let first = consolidate::trigger_consolidation(&conn, "manual").unwrap();
    assert_eq!(first.conflict_count, 1);
    assert_eq!(first.conflicts.len(), 1);

    let second = consolidate::trigger_consolidation(&conn, "manual").unwrap();
    assert_eq!(second.conflict_count, 0, "同一对节点不重复生成洞察");
    let insights: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM insights WHERE kind = 'conflict'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(insights, 1);

    let runs = consolidate::list_consolidation_runs(&conn, 10).unwrap();
    assert_eq!(runs.len(), 2);
    let stored = consolidate::get_consolidation_report(&conn, &first.run_id).unwrap();
    assert_eq!(stored.conflict_count, 1);
    assert_eq!(stored.mode, "manual");
}

#[test]
fn consolidation_decays_stale_low_weight_edge() {
    let conn = db();
    let a = add(&conn, NodeKind::Idea, "陈旧甲", "test");
    let b = add(&conn, NodeKind::Idea, "陈旧乙", "test");
    let edge = repo::link_nodes(&conn, &a, &b, Relation::Analogous, 0.5).unwrap();
    conn.execute(
        "UPDATE thought_edges SET weight = 0.01, last_activated_at = NULL WHERE id = ?1",
        [edge.edge_id.as_str()],
    )
    .unwrap();

    let report = consolidate::trigger_consolidation(&conn, "manual").unwrap();
    assert!(report.decayed_count >= 1);
    let status: String = conn
        .query_row(
            "SELECT status FROM thought_edges WHERE id = ?1",
            [edge.edge_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "stale");

    let again = consolidate::trigger_consolidation(&conn, "manual").unwrap();
    assert_eq!(again.decayed_count, 0, "状态跃迁只发生一次");
}

#[test]
fn graph_limit_is_clamped() {
    let conn = db();
    for index in 0..5 {
        add(&conn, NodeKind::Idea, &format!("观点 {index}"), "test");
    }
    let view = repo::get_graph(
        &conn,
        &GraphFilter {
            limit: Some(DEFAULT_GRAPH_LIMIT),
            ..GraphFilter::default()
        },
    )
    .unwrap();
    assert_eq!(view.nodes.len(), 5);
    assert!(!view.truncated);
}

// ---------- 属性测试（proptest） ----------

/// 八条内容彼此差异明显，避免随机组合时被固化误判为相似节点。
const DISTINCT: [&str; 8] = [
    "注意力是稀缺资源",
    "长期主义依赖耐心",
    "先算清最坏结果",
    "情绪是信号的载体",
    "工具会放大判断力",
    "时机决定多数成败",
    "复利来自持续投入",
    "边界比目标更能约束行动",
];

/// 建 `count` 个互不相同的观点节点，返回它们的 id。
fn build_nodes(conn: &rusqlite::Connection, count: usize) -> Vec<String> {
    (0..count.max(1).min(DISTINCT.len()))
        .map(|index| add(conn, NodeKind::Idea, DISTINCT[index], "prop"))
        .collect()
}

/// 网络规模三元组，用于固化幂等断言。
fn network_counts(conn: &rusqlite::Connection) -> (i64, i64, i64) {
    let nodes = conn
        .query_row("SELECT COUNT(*) FROM thought_nodes", [], |row| row.get(0))
        .unwrap();
    let edges = conn
        .query_row("SELECT COUNT(*) FROM thought_edges", [], |row| row.get(0))
        .unwrap();
    let insights = conn
        .query_row("SELECT COUNT(*) FROM insights", [], |row| row.get(0))
        .unwrap();
    (nodes, edges, insights)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// 属性 13：一次唤醒内，被唤起节点的激发后激活度不低于激发前。
    #[test]
    fn property_activation_never_reduces(
        count in 1usize..=DISTINCT.len(),
        prime in 0.5f64..2.0,
        wake in 0.5f64..2.0,
    ) {
        let conn = db();
        let ids = build_nodes(&conn, count);
        repo::activate(&conn, &ids, prime, None).unwrap();
        let before: Vec<f64> = ids
            .iter()
            .map(|id| repo::get_node(&conn, id).unwrap().node.activation)
            .collect();

        repo::activate(&conn, &ids, wake, None).unwrap();
        for (index, id) in ids.iter().enumerate() {
            let after = repo::get_node(&conn, id).unwrap().node.activation;
            prop_assert!(after >= before[index], "节点 {id} 激发后回落");
        }
    }

    /// 属性 11：每次会诊的结论、框架与分歧都写入网络并连成对应关系。
    #[test]
    fn property_council_converges_into_network(
        frameworks in 1usize..=4,
        divergences in 0usize..=3,
    ) {
        let conn = db();
        let session = council::create_session(
            &conn,
            "如何在扩张与收敛之间取舍",
            &["growth".to_string()],
            &[Layer::Dao, Layer::Shu],
            Strategy::Steady,
        )
        .unwrap();
        let answers: Vec<String> = (0..frameworks)
            .map(|index| format!("第{index}种框架：{}", DISTINCT[index]))
            .collect();
        for (index, answer) in answers.iter().enumerate() {
            council::save_turn(
                &conn,
                &council::NewTurn {
                    session_id: session.clone(),
                    round: 1,
                    panel_rotation: 0,
                    role: "answer".to_string(),
                    master_id: Some(format!("master-{index}")),
                    master_version: Some(1),
                    content: answer.clone(),
                    citations: Vec::new(),
                    prompt_version: "2026-09-15.1".to_string(),
                    status: "ok".to_string(),
                    error_code: None,
                },
            )
            .unwrap();
        }
        let splits: Vec<String> = (0..divergences)
            .map(|index| format!("分歧{index}：{}", DISTINCT[index + 4]))
            .collect();
        council::finish_session(&conn, &session, "先收敛现金流，再按节奏扩张", &splits).unwrap();

        let outcome = recorder::record_session(&conn, &session).unwrap();
        prop_assert_eq!(outcome.framework_ids.len(), frameworks);
        prop_assert_eq!(outcome.divergence_ids.len(), divergences);

        let detail = repo::get_node(&conn, &outcome.judgment_id).unwrap();
        prop_assert_eq!(detail.node.kind, NodeKind::Judgment);
        for framework in &outcome.framework_ids {
            prop_assert!(
                detail.links.iter().any(|link| link.peer_id == *framework),
                "框架 {framework} 未连到判断"
            );
        }
        for divergence in &outcome.divergence_ids {
            prop_assert!(
                detail
                    .links
                    .iter()
                    .any(|link| link.peer_id == *divergence && link.relation == Relation::Conflicts),
                "分歧 {divergence} 未连成冲突关系"
            );
        }
    }

    /// 属性 14：同一批输入重复固化不产生重复连线、节点或洞察。
    #[test]
    fn property_consolidation_is_idempotent(
        count in 2usize..=DISTINCT.len(),
        rotate in 0usize..16,
    ) {
        let conn = db();
        let ids = build_nodes(&conn, count);
        let offset = rotate % ids.len();
        for index in 0..ids.len() {
            let next = (index + offset + 1) % ids.len();
            if ids[index] != ids[next] {
                repo::link_nodes(&conn, &ids[index], &ids[next], Relation::Supports, 0.4).unwrap();
            }
        }
        repo::activate(&conn, &ids, 1.0, None).unwrap();

        consolidate::trigger_consolidation(&conn, "manual").unwrap();
        let before = network_counts(&conn);
        let second = consolidate::trigger_consolidation(&conn, "idle").unwrap();

        prop_assert_eq!(second.strengthened_count, 0);
        prop_assert_eq!(second.merged_count, 0);
        prop_assert_eq!(second.conflict_count, 0);
        prop_assert_eq!(network_counts(&conn), before, "重复固化改变了网络规模");
    }

    /// 属性 15：同一图数据下的多跳传播可复现，且远端增益严格小于近端。
    #[test]
    fn property_multi_hop_propagation_is_reproducible(
        length in 2usize..=5,
        weight in 0.2f64..0.95,
    ) {
        fn chain(length: usize, weight: f64) -> (rusqlite::Connection, Vec<String>) {
            let conn = db();
            let ids = build_nodes(&conn, length);
            for index in 0..ids.len() - 1 {
                repo::link_nodes(
                    &conn,
                    &ids[index],
                    &ids[index + 1],
                    Relation::Supports,
                    weight,
                )
                .unwrap();
            }
            (conn, ids)
        }

        let (first, ids) = chain(length, weight);
        let (second, other) = chain(length, weight);
        let outcome = repo::activate(&first, &[ids[0].clone()], 1.0, None).unwrap();
        let repeat = repo::activate(&second, &[other[0].clone()], 1.0, None).unwrap();

        prop_assert_eq!(outcome.propagated, repeat.propagated);
        prop_assert_eq!(outcome.propagated_far, repeat.propagated_far);
        prop_assert_eq!(outcome.hops, repeat.hops);
        prop_assert_eq!(outcome.propagated, 1, "链首激活只唤醒一个近邻");
        if length >= 3 {
            prop_assert_eq!(outcome.propagated_far, 1);
            let near = repo::get_node(&first, &ids[1]).unwrap().node.activation;
            let far = repo::get_node(&first, &ids[2]).unwrap().node.activation;
            prop_assert!(far > 0.0 && far < near, "远端增益未按跳间衰减");
        }
    }
}
