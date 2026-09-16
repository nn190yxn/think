//! 认知社区检测：确定性标签传播与标签生成。
//!
//! 迭代顺序与平票规则都按节点标识确定，因此同一图数据与同一参数下，
//! 社区划分与标签必然一致。社区只在固化时重算，读取路径保持廉价。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::error::CoreResult;
use crate::util::unique_id;

/// 标签传播的迭代上限，超过即停止，避免在稠密图上长时间占用。
pub const MAX_ITERATIONS: usize = 20;

/// 一个认知社区。
#[derive(Debug, Clone)]
pub struct Community {
    pub id: String,
    pub label: String,
    pub domain: String,
    pub layer: String,
    pub members: Vec<String>,
}

struct NodeMeta {
    id: String,
    domains: Vec<String>,
    layers: Vec<String>,
}

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

/// 取出现次数最多且标识最小的取值，用于领域与层次的众数。
fn mode(values: &[String]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for value in values {
        *counts.entry(value.as_str()).or_insert(0) += 1;
    }
    counts
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
        .map(|(value, _)| value.to_string())
        .unwrap_or_default()
}

/// 按成员占比生成社区标签：先领域、再层次、最后「未归类」。
fn label_meta(members: &[&NodeMeta]) -> (String, String, String) {
    let domains: Vec<String> = members
        .iter()
        .flat_map(|member| member.domains.clone())
        .collect();
    let layers: Vec<String> = members
        .iter()
        .flat_map(|member| member.layers.clone())
        .collect();

    let domain = mode(&domains);
    if !domain.is_empty() {
        return (domain.clone(), domain, String::new());
    }
    let layer = mode(&layers);
    if !layer.is_empty() {
        // 标签是给人看的，层次用中文名；`layer` 字段保留键以便前端映射配色。
        let label = crate::master::Layer::parse(&layer)
            .map(|layer| layer.name().to_string())
            .unwrap_or_else(|| layer.clone());
        return (label, String::new(), layer);
    }
    ("未归类".to_string(), String::new(), String::new())
}

/// 检测社区。成员数小于 `min_size` 的社区不返回。
pub fn detect(conn: &Connection, min_size: i64, min_edge_weight: f64) -> CoreResult<Vec<Community>> {
    let mut meta_stmt = conn.prepare(
        "SELECT id, domains_json, layers_json FROM thought_nodes
         WHERE superseded_by IS NULL ORDER BY id ASC",
    )?;
    let meta_rows = meta_stmt.query_map([], |row| {
        Ok(NodeMeta {
            id: row.get(0)?,
            domains: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
            layers: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
        })
    })?;
    let mut metas: Vec<NodeMeta> = Vec::new();
    for row in meta_rows {
        metas.push(row?);
    }
    drop(meta_stmt);

    if metas.is_empty() {
        return Ok(Vec::new());
    }

    let ids: BTreeSet<String> = metas.iter().map(|meta| meta.id.clone()).collect();
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    {
        let mut edge_stmt = conn.prepare(
            "SELECT from_node_id, to_node_id FROM thought_edges
             WHERE status = 'active' AND weight >= ?1
             ORDER BY from_node_id ASC, to_node_id ASC",
        )?;
        let edge_rows = edge_stmt.query_map([min_edge_weight], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut pairs: Vec<(String, String)> = Vec::new();
        for row in edge_rows {
            pairs.push(row?);
        }
        drop(edge_stmt);
        for (from, to) in pairs {
            if !ids.contains(&from) || !ids.contains(&to) {
                continue;
            }
            adjacency.entry(from.clone()).or_default().insert(to.clone());
            adjacency.entry(to).or_default().insert(from);
        }
    }

    // 初始时每个节点持有自身标识作为标签。
    let mut labels: BTreeMap<String, String> = metas
        .iter()
        .map(|meta| (meta.id.clone(), meta.id.clone()))
        .collect();

    for _ in 0..MAX_ITERATIONS {
        let mut changed = false;
        for meta in &metas {
            let Some(neighbours) = adjacency.get(&meta.id) else {
                continue;
            };
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for neighbour in neighbours {
                let label = labels.get(neighbour).map(String::as_str).unwrap_or(neighbour);
                *counts.entry(label).or_insert(0) += 1;
            }
            // 出现次数最多且标识最小的标签；平票取标识较小者。
            if let Some((best, _)) = counts
                .iter()
                .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
            {
                if labels.get(&meta.id).map(String::as_str) != Some(*best) {
                    labels.insert(meta.id.clone(), (*best).to_string());
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // 按标签归组，成员保持标识升序。
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for meta in &metas {
        let label = labels
            .get(&meta.id)
            .cloned()
            .unwrap_or_else(|| meta.id.clone());
        grouped.entry(label).or_default().push(meta.id.clone());
    }

    let meta_index: BTreeMap<&str, &NodeMeta> =
        metas.iter().map(|meta| (meta.id.as_str(), meta)).collect();
    let mut communities: Vec<Community> = Vec::new();
    for members in grouped.into_values() {
        if (members.len() as i64) < min_size {
            continue;
        }
        let member_metas: Vec<&NodeMeta> = members
            .iter()
            .filter_map(|id| meta_index.get(id.as_str()).copied())
            .collect();
        let (label, domain, layer) = label_meta(&member_metas);
        let seed = format!("{label}:{}", members.join("|"));
        communities.push(Community {
            id: unique_id("cluster", &seed),
            label,
            domain,
            layer,
            members,
        });
    }
    communities.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then(left.members.first().cmp(&right.members.first()))
    });
    Ok(communities)
}

/// 写入社区并把成员节点的 `cluster_id` 指向本次社区。调用方需自带事务。
pub fn persist(conn: &Connection, run_id: &str, communities: &[Community]) -> CoreResult<i64> {
    let timestamp = now(conn)?;
    conn.execute(
        "UPDATE thought_nodes SET cluster_id = NULL WHERE cluster_id IS NOT NULL",
        [],
    )?;
    for community in communities {
        conn.execute(
            "INSERT INTO thought_clusters
                 (id, run_id, label, domain, layer, member_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                community.id,
                run_id,
                community.label,
                community.domain,
                community.layer,
                community.members.len() as i64,
                timestamp,
            ],
        )?;
        for member in &community.members {
            conn.execute(
                "UPDATE thought_nodes SET cluster_id = ?2 WHERE id = ?1",
                rusqlite::params![member, community.id],
            )?;
        }
    }
    Ok(communities.len() as i64)
}
