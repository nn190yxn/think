//! 迭代闭环：主题聚合、连续采纳提升为原则、年轮概览。

use rusqlite::Connection;

use crate::error::CoreResult;
use crate::master::Layer;
use crate::network::recorder::SOURCE_RECORD;
use crate::network::repo as network_repo;
use crate::network::repo::MAX_GRAPH_LIMIT;
use crate::network::{GraphFilter, NodeKind, Relation};

use super::{
    repo, DomainTrend, PrincipleSeal, RingOverview, TopicView, PRINCIPLE_ADOPTION_THRESHOLD,
};

/// 原则节点的来源标记。用 topic_key 作为 source_ref，便于回溯源起主题。
pub const SOURCE_PRINCIPLE: &str = "principle_promotion";
/// 判断派生出个人原则的权重。
const PRINCIPLE_LINK_WEIGHT: f64 = 0.8;
/// 领域增长统计的时间窗（天）。
const DOMAIN_WINDOW_DAYS: i64 = 30;
/// 新增连线统计的时间窗（天）。
const EDGE_WINDOW_DAYS: i64 = 7;

fn parse_strings(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn parse_layers(raw: &str) -> Vec<Layer> {
    let names: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    let mut layers: Vec<Layer> = names.iter().filter_map(|name| Layer::parse(name)).collect();
    layers.sort();
    layers.dedup();
    layers
}

/// 同一 topic_key 下的思考记录聚合，时间倒序。
pub fn list_topics(conn: &Connection, limit: i64) -> CoreResult<Vec<TopicView>> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT r.topic_key,
                COUNT(*) AS record_count,
                SUM(r.adopted) AS adopted_count,
                (SELECT conclusion FROM thought_records latest
                  WHERE latest.topic_key = r.topic_key
                  ORDER BY latest.created_at DESC, latest.rowid DESC LIMIT 1) AS latest_conclusion,
                MAX(r.created_at) AS latest_at,
                MIN(r.created_at) AS created_at
         FROM thought_records r
         GROUP BY r.topic_key
         ORDER BY latest_at DESC, r.topic_key ASC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(TopicView {
            topic_key: row.get(0)?,
            record_count: row.get(1)?,
            adopted_count: row.get(2)?,
            latest_conclusion: row.get(3)?,
            latest_at: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    let mut topics = Vec::new();
    for row in rows {
        topics.push(row?);
    }
    Ok(topics)
}

/// 从最新一条记录起连续采纳的次数。
fn consecutive_adopted(conn: &Connection, topic_key: &str) -> CoreResult<i64> {
    let mut stmt = conn.prepare(
        "SELECT adopted FROM thought_records
         WHERE topic_key = ?1
         ORDER BY created_at DESC, rowid DESC",
    )?;
    let rows = stmt.query_map([topic_key], |row| row.get::<_, i64>(0))?;
    let mut count = 0i64;
    for row in rows {
        if row? != 0 {
            count += 1;
        } else {
            break;
        }
    }
    Ok(count)
}

/// 把连续采纳达到阈值的主题提升为个人原则节点。
///
/// 原判断节点保留，并新增一条指向原则节点的衍生连线；同一主题只提升一次。
pub fn promote_principles(conn: &Connection) -> CoreResult<Vec<String>> {
    let topics = list_topics(conn, 200)?;
    let mut promoted = Vec::new();

    for topic in topics {
        if consecutive_adopted(conn, &topic.topic_key)? < PRINCIPLE_ADOPTION_THRESHOLD {
            continue;
        }
        let existing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM thought_nodes
             WHERE kind = 'principle' AND source_kind = ?1 AND source_ref = ?2
               AND superseded_by IS NULL",
            rusqlite::params![SOURCE_PRINCIPLE, topic.topic_key],
            |row| row.get(0),
        )?;
        if existing > 0 {
            continue;
        }

        // 取该主题最新一条记录的判断节点作为原则的来源。
        let latest: Option<(String, String, String)> = conn
            .query_row(
                "SELECT id, domains_json, layers_json FROM thought_records
                 WHERE topic_key = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [topic.topic_key.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();
        let Some((record_id, domains_json, layers_json)) = latest else {
            continue;
        };
        let Some(judgment_id) = network_repo::find_by_source(conn, SOURCE_RECORD, &record_id)?
        else {
            continue;
        };

        let domains = parse_strings(&domains_json);
        let layers = parse_layers(&layers_json);
        let principle = network_repo::upsert_node(
            conn,
            &network_repo::NewNode {
                kind: NodeKind::Principle,
                content: &topic.latest_conclusion,
                source_kind: SOURCE_PRINCIPLE,
                source_ref: &topic.topic_key,
                domains: &domains,
                layers: &layers,
            },
        )?;
        if principle.node_id == judgment_id {
            // 内容与判断完全一致时若被判为同一节点，跳过，避免自连。
            continue;
        }
        network_repo::link_nodes(
            conn,
            &judgment_id,
            &principle.node_id,
            Relation::Derives,
            PRINCIPLE_LINK_WEIGHT,
        )?;
        let _ = network_repo::activate(conn, std::slice::from_ref(&principle.node_id), 1.0, None)?;
        promoted.push(principle.node_id);
    }

    Ok(promoted)
}

/// 已沉淀的个人原则。
pub fn list_principles(conn: &Connection, limit: i64) -> CoreResult<Vec<PrincipleSeal>> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT id, content, domains_json, layers_json, activation, source_ref, created_at
         FROM thought_nodes
         WHERE kind = 'principle' AND source_kind = ?1 AND superseded_by IS NULL
           AND status = 'active'
         ORDER BY activation DESC, created_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![SOURCE_PRINCIPLE, limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut seals = Vec::new();
    for row in rows {
        let (node_id, content, domains_json, layers_json, activation, topic_key, created_at) = row?;
        seals.push(PrincipleSeal {
            node_id,
            content,
            domains: parse_strings(&domains_json),
            layers: parse_layers(&layers_json),
            activation,
            adopted_count: consecutive_adopted(conn, &topic_key)?,
            created_at,
        });
    }
    Ok(seals)
}

/// 年轮概览：激活度最高的节点、增长最快的领域、新增连线数与原则数。
pub fn ring_overview(conn: &Connection) -> CoreResult<RingOverview> {
    let graph = network_repo::get_graph(
        conn,
        &GraphFilter {
            limit: Some(6),
            ..GraphFilter::default()
        },
    )?;

    let nodes = network_repo::list_nodes(
        conn,
        &GraphFilter {
            limit: Some(MAX_GRAPH_LIMIT),
            ..GraphFilter::default()
        },
    )?;
    let cutoff: String = conn.query_row(
        &format!("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-{DOMAIN_WINDOW_DAYS} days')"),
        [],
        |row| row.get(0),
    )?;
    let mut domain_counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for node in &nodes {
        if node.created_at < cutoff {
            continue;
        }
        for domain in &node.domains {
            *domain_counts.entry(domain.clone()).or_insert(0) += 1;
        }
    }
    let mut fastest: Vec<DomainTrend> = domain_counts
        .into_iter()
        .map(|(domain, count)| DomainTrend { domain, count })
        .collect();
    fastest.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.domain.cmp(&b.domain)));
    fastest.truncate(4);

    let edge_cutoff: String = conn.query_row(
        &format!("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-{EDGE_WINDOW_DAYS} days')"),
        [],
        |row| row.get(0),
    )?;
    let new_edge_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM thought_edges WHERE created_at >= ?1",
        [edge_cutoff.as_str()],
        |row| row.get(0),
    )?;
    let principle_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM thought_nodes
         WHERE kind = 'principle' AND superseded_by IS NULL AND status = 'active'",
        [],
        |row| row.get(0),
    )?;

    Ok(RingOverview {
        top_nodes: graph.nodes,
        fastest_domains: fastest,
        new_edge_count,
        principle_count,
    })
}

/// 待处置洞察数，供余烬入口展示。
pub fn pending_insights(conn: &Connection) -> CoreResult<i64> {
    repo::pending_count(conn)
}
