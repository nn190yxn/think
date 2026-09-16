//! 认知社区：确定性划分、标签回退、最小规模过滤、落库与历史保留。

use proptest::prelude::*;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::master::Layer;
use thought_forge_core::network::cluster;
use thought_forge_core::network::repo::{self, NewNode};
use thought_forge_core::network::{consolidate, NodeKind, Relation};

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn add(
    conn: &rusqlite::Connection,
    content: &str,
    domains: &[String],
    layers: &[Layer],
) -> String {
    repo::upsert_node(
        conn,
        &NewNode {
            kind: NodeKind::Idea,
            content,
            source_kind: "test",
            source_ref: "test",
            domains,
            layers,
        },
    )
    .expect("节点可写入")
    .node_id
}

/// 划分的等价形式：标签与成员集合，忽略每次生成的社区标识。
fn partition(communities: &[cluster::Community]) -> Vec<(String, Vec<String>)> {
    let mut parts: Vec<(String, Vec<String>)> = communities
        .iter()
        .map(|community| (community.label.clone(), community.members.clone()))
        .collect();
    parts.sort();
    parts
}

#[test]
fn detect_is_reproducible_for_the_same_graph() {
    let conn = db();
    let a = add(&conn, "注意力是稀缺资源", &["growth".into()], &[Layer::Qi]);
    let b = add(&conn, "把时间花在复利上", &["growth".into()], &[Layer::Qi]);
    let c = add(&conn, "定价是价值的表达", &["business".into()], &[Layer::Fa]);
    let d = add(&conn, "价格由供需决定", &["business".into()], &[Layer::Fa]);
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.8).unwrap();
    repo::link_nodes(&conn, &c, &d, Relation::Supports, 0.7).unwrap();

    let first = cluster::detect(&conn, 2, 0.05).unwrap();
    let second = cluster::detect(&conn, 2, 0.05).unwrap();
    assert_eq!(partition(&first), partition(&second), "同一图数据划分一致");
    assert_eq!(first.len(), 2, "两组连线形成两个社区");
    assert!(first.iter().any(|community| community.domain == "growth"));
    assert!(first.iter().any(|community| community.domain == "business"));
}

#[test]
fn min_size_filters_small_groups() {
    let conn = db();
    let a = add(&conn, "独立的念头", &["self".into()], &[Layer::Dao]);
    let b = add(&conn, "另一个孤立念头", &["self".into()], &[Layer::Dao]);
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.9).unwrap();

    assert_eq!(cluster::detect(&conn, 2, 0.05).unwrap().len(), 1);
    assert!(cluster::detect(&conn, 3, 0.05).unwrap().is_empty());
}

#[test]
fn label_falls_back_to_layer_then_unclassified() {
    let conn = db();
    let a = add(&conn, "只带层次的念头甲", &[], &[Layer::Shu]);
    let b = add(&conn, "只带层次的念头乙", &[], &[Layer::Shu]);
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.9).unwrap();
    let by_layer = cluster::detect(&conn, 2, 0.05).unwrap();
    assert_eq!(by_layer.len(), 1);
    assert_eq!(by_layer[0].label, "术");
    assert_eq!(by_layer[0].layer, "shu");
    assert_eq!(by_layer[0].domain, "");

    let conn = db();
    let c = add(&conn, "既无领域也无层次甲", &[], &[]);
    let d = add(&conn, "既无领域也无层次乙", &[], &[]);
    repo::link_nodes(&conn, &c, &d, Relation::Supports, 0.9).unwrap();
    let unclassified = cluster::detect(&conn, 2, 0.05).unwrap();
    assert_eq!(unclassified[0].label, "未归类");
    assert_eq!(unclassified[0].domain, "");
    assert_eq!(unclassified[0].layer, "");
}

#[test]
fn low_weight_edges_do_not_join_communities() {
    let conn = db();
    let a = add(&conn, "甲", &[], &[]);
    let b = add(&conn, "乙", &[], &[]);
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.01).unwrap();
    assert!(cluster::detect(&conn, 2, 0.5).unwrap().is_empty());
    assert_eq!(cluster::detect(&conn, 2, 0.005).unwrap().len(), 1);
}

#[test]
fn persist_points_nodes_at_the_latest_community_and_keeps_history() {
    let conn = db();
    let a = add(&conn, "甲", &["growth".into()], &[]);
    let b = add(&conn, "乙", &["growth".into()], &[]);
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.9).unwrap();

    let first = cluster::detect(&conn, 2, 0.05).unwrap();
    let count = cluster::persist(&conn, "run-1", &first).unwrap();
    assert_eq!(count, 1);

    // 再来一次：上一批社区行保留，节点指向本次社区。
    let second = cluster::detect(&conn, 2, 0.05).unwrap();
    cluster::persist(&conn, "run-2", &second).unwrap();
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM thought_clusters", [], |row| row.get(0))
        .unwrap();
    assert_eq!(total, 2, "历史社区保留");

    let node = repo::get_node(&conn, &a).unwrap().node;
    assert_eq!(node.cluster_id.as_deref(), Some(second[0].id.as_str()));
}

#[test]
fn graph_exposes_clusters_and_supports_filtering() {
    let conn = db();
    let a = add(&conn, "甲", &["growth".into()], &[]);
    let b = add(&conn, "乙", &["growth".into()], &[]);
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.9).unwrap();
    let communities = cluster::detect(&conn, 2, 0.05).unwrap();
    cluster::persist(&conn, "run-1", &communities).unwrap();

    let graph = repo::get_graph(&conn, &thought_forge_core::network::GraphFilter::default()).unwrap();
    assert_eq!(graph.clusters.len(), 1);
    assert_eq!(graph.clusters[0].member_count, 2);

    let filtered = repo::get_graph(
        &conn,
        &thought_forge_core::network::GraphFilter {
            cluster_id: Some(communities[0].id.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(filtered.nodes.len(), 2);
}

#[test]
fn consolidation_clusters_when_enabled() {
    let conn = db();
    let a = add(&conn, "甲", &["growth".into()], &[]);
    let b = add(&conn, "乙", &["growth".into()], &[]);
    repo::link_nodes(&conn, &a, &b, Relation::Supports, 0.9).unwrap();
    thought_forge_core::council::tuning::set(
        &conn,
        &[("network.cluster_min_size".to_string(), "2".to_string())],
    )
    .unwrap();

    let report = consolidate::trigger_consolidation(&conn, "manual").unwrap();
    assert_eq!(report.cluster_count, 1);

    // 关闭聚类后不再重算，既有归属保持不变。
    thought_forge_core::council::tuning::set(
        &conn,
        &[("network.cluster_enabled".to_string(), "false".to_string())],
    )
    .unwrap();
    let report = consolidate::trigger_consolidation(&conn, "idle").unwrap();
    assert_eq!(report.cluster_count, 0);
    assert!(repo::get_node(&conn, &a).unwrap().node.cluster_id.is_some());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// property 9：同一图数据与同一参数下，社区划分与标签一致。
    #[test]
    fn detection_is_deterministic(edges in prop::collection::vec((0usize..6, 0usize..6), 0..8)) {
        let conn = db();
        let ids: Vec<String> = (0..6)
            .map(|index| add(&conn, &format!("节点{index}"), &["d".into()], &[]))
            .collect();
        for (from, to) in &edges {
            if from == to {
                continue;
            }
            let _ = repo::link_nodes(&conn, &ids[*from], &ids[*to], Relation::Supports, 0.6);
        }
        let first = cluster::detect(&conn, 2, 0.05).unwrap();
        let second = cluster::detect(&conn, 2, 0.05).unwrap();
        prop_assert_eq!(partition(&first), partition(&second));
    }
}
