//! 思维网络仓储：节点、连线、激活、衰减、图谱与冲突裁决。

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};

use crate::error::{CoreError, CoreResult};
use crate::master::Layer;
use crate::util::unique_id;

use super::{
    ActivationOutcome, ActivationView, ClusterView, EdgeUpsert, GraphEdge, GraphFilter, GraphNode,
    GraphView, NodeDetail, NodeKind, NodeLink, NodeUpsert, Relation, ThoughtNode,
    DEFAULT_HALF_LIFE_HOURS, HALF_LIFE_SETTING_KEY, ThoughtRecordView,
};

/// 图谱节点上限的安全区间，避免一次查询把整张图读进内存。
pub const MIN_GRAPH_LIMIT: i64 = 50;
pub const MAX_GRAPH_LIMIT: i64 = 4000;
pub const DEFAULT_GRAPH_LIMIT: i64 = 800;

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String = conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
        row.get(0)
    })?;
    Ok(value)
}

/// 归一化内容：去掉空白与常见标点并转小写，用于同内容判重。
pub fn normalize(content: &str) -> String {
    content
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .filter(|ch| !"，。！？；：、（）「」『』《》,.!?;:()[]{}<>\"'".contains(*ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn json_strings(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

fn layers_json(layers: &[Layer]) -> String {
    let names: Vec<&str> = layers.iter().map(|layer| layer.as_str()).collect();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
}

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

fn parse_kind(raw: &str) -> NodeKind {
    NodeKind::parse(raw).unwrap_or(NodeKind::Idea)
}

fn parse_relation(raw: &str) -> Relation {
    Relation::parse(raw).unwrap_or(Relation::Supports)
}

/// 某个时间点距今的小时数，负数按 0 处理。
fn elapsed_hours(conn: &Connection, timestamp: &str) -> CoreResult<f64> {
    let hours: f64 = conn.query_row(
        "SELECT MAX(0.0, (julianday('now') - julianday(?1)) * 24.0)",
        [timestamp],
        |row| row.get(0),
    )?;
    Ok(hours)
}

/// 半衰期（小时）。设置缺失或非法时退回默认七天。
pub fn half_life_hours(conn: &Connection) -> CoreResult<f64> {
    let raw = crate::db::settings::get(conn, HALF_LIFE_SETTING_KEY)?;
    let parsed = raw.and_then(|value| value.parse::<f64>().ok());
    Ok(match parsed {
        Some(hours) if hours > 0.0 => hours,
        _ => DEFAULT_HALF_LIFE_HOURS,
    })
}

/// 时间衰减系数：0.5 的 (elapsed / half_life) 次幂。
pub fn decay_factor(elapsed_hours: f64, half_life_hours: f64) -> f64 {
    if elapsed_hours <= 0.0 || half_life_hours <= 0.0 {
        return 1.0;
    }
    0.5_f64.powf(elapsed_hours / half_life_hours)
}

/// 写入节点的输入。
pub struct NewNode<'a> {
    pub kind: NodeKind,
    pub content: &'a str,
    pub source_kind: &'a str,
    pub source_ref: &'a str,
    pub domains: &'a [String],
    pub layers: &'a [Layer],
}

/// 写入节点。同类型同内容视为同一节点，合并来源与标签。
pub fn upsert_node(conn: &Connection, input: &NewNode<'_>) -> CoreResult<NodeUpsert> {
    let content = input.content.trim();
    if content.is_empty() {
        return Err(CoreError::InvalidInput("节点内容不能为空".into()));
    }
    let normalized = normalize(content);
    if normalized.is_empty() {
        return Err(CoreError::InvalidInput("节点内容去掉标点后为空".into()));
    }

    let existing: Option<(String, String, String)> = conn
        .query_row(
            "SELECT id, domains_json, layers_json FROM thought_nodes
             WHERE kind = ?1 AND normalized_content = ?2 AND superseded_by IS NULL",
            rusqlite::params![input.kind.as_str(), normalized],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    if let Some((id, stored_domains, stored_layers)) = existing {
        let mut domains = parse_strings(&stored_domains);
        for domain in input.domains {
            if !domains.contains(domain) {
                domains.push(domain.clone());
            }
        }
        domains.sort();
        let mut layers = parse_layers(&stored_layers);
        for layer in input.layers {
            if !layers.contains(layer) {
                layers.push(*layer);
            }
        }
        layers.sort();
        conn.execute(
            "UPDATE thought_nodes SET domains_json = ?2, layers_json = ?3 WHERE id = ?1",
            rusqlite::params![id, json_strings(&domains), layers_json(&layers)],
        )?;
        return Ok(NodeUpsert {
            node_id: id,
            outcome: "matched".to_string(),
        });
    }

    let timestamp = now(conn)?;
    let id = unique_id("node", &format!("{}:{}", input.kind.as_str(), normalized));
    conn.execute(
        "INSERT INTO thought_nodes
             (id, kind, content, normalized_content, source_kind, source_ref, domains_json,
              layers_json, activation, activation_updated_at, version, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, 1, ?9)",
        rusqlite::params![
            id,
            input.kind.as_str(),
            content,
            normalized,
            input.source_kind,
            input.source_ref,
            json_strings(input.domains),
            layers_json(input.layers),
            timestamp,
        ],
    )?;
    Ok(NodeUpsert {
        node_id: id,
        outcome: "created".to_string(),
    })
}

/// 建立或增强连线。冲突关系按节点 id 排序归一化，保证双向调用命中同一条记录。
pub fn link_nodes(
    conn: &Connection,
    from: &str,
    to: &str,
    relation: Relation,
    weight: f64,
) -> CoreResult<EdgeUpsert> {
    if from == to {
        return Err(CoreError::InvalidInput("节点不能与自己连线".into()));
    }
    ensure_node(conn, from)?;
    ensure_node(conn, to)?;

    let (from_id, to_id) = if relation.is_symmetric() && from > to {
        (to.to_string(), from.to_string())
    } else {
        (from.to_string(), to.to_string())
    };
    let weight = weight.clamp(0.0, 1.0);

    let existing: Option<(String, f64)> = conn
        .query_row(
            "SELECT id, weight FROM thought_edges
             WHERE from_node_id = ?1 AND to_node_id = ?2 AND relation = ?3",
            rusqlite::params![from_id, to_id, relation.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let timestamp = now(conn)?;
    if let Some((id, current)) = existing {
        // 重复声明同一条关系时取较大权重，不新增记录。
        let next = current.max(weight);
        conn.execute(
            "UPDATE thought_edges SET weight = ?2, status = 'active' WHERE id = ?1",
            rusqlite::params![id, next],
        )?;
        return Ok(EdgeUpsert {
            edge_id: id,
            created: false,
            weight: next,
        });
    }

    let id = unique_id("edge", &format!("{from_id}->{to_id}:{}", relation.as_str()));
    conn.execute(
        "INSERT INTO thought_edges
             (id, from_node_id, to_node_id, relation, weight, co_activation_count, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 'active', ?6)",
        rusqlite::params![id, from_id, to_id, relation.as_str(), weight, timestamp],
    )?;
    Ok(EdgeUpsert {
        edge_id: id,
        created: true,
        weight,
    })
}

fn ensure_node(conn: &Connection, node_id: &str) -> CoreResult<()> {
    let exists: Option<String> = conn
        .query_row("SELECT id FROM thought_nodes WHERE id = ?1", [node_id], |row| {
            row.get(0)
        })
        .optional()?;
    match exists {
        Some(_) => Ok(()),
        None => Err(CoreError::NotFound(format!("网络节点 {node_id}"))),
    }
}

/// 提升节点及其多跳邻居的激活度，并记录唤起时间。
///
/// 激发前先把存储值按经过时间衰减到当前，再加增量，因此
/// `activation = decayed + increment` 恒不低于激发前的真实值。
/// 传播跳数、折半系数与跳间衰减取自参数服务，遍历顺序按节点标识
/// 升序，保证同一图数据与同一输入得到同样的结果。
pub fn activate(
    conn: &Connection,
    node_ids: &[String],
    increment: f64,
    session_id: Option<&str>,
) -> CoreResult<ActivationOutcome> {
    let increment = increment.max(0.0);
    let half_life = half_life_hours(conn)?;
    let timestamp = now(conn)?;
    let tuning = crate::council::tuning::snapshot(conn)?;
    let hop_count = tuning.hop_count.clamp(1, 3) as usize;

    let mut direct: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for id in node_ids {
        if seen.insert(id.clone()) {
            direct.push(id.clone());
        }
    }

    let mut activated = 0usize;
    for id in &direct {
        if apply_activation(conn, id, increment, &half_life, &timestamp, session_id)? {
            activated += 1;
        }
    }

    // 一跳传播：邻居按连线权重折半增益，并累计共激活次数。
    let mut touched_edges: Vec<(String, String, f64)> = Vec::new();
    for id in &direct {
        let neighbours = neighbours_of(conn, id)?;
        for (edge_id, peer_id, weight) in neighbours {
            touched_edges.push((edge_id, peer_id, weight));
        }
    }
    let mut first_hop: std::collections::BTreeMap<String, f64> =
        std::collections::BTreeMap::new();
    for (edge_id, peer_id, weight) in touched_edges {
        bump_edge(conn, &edge_id, &timestamp)?;
        if seen.contains(&peer_id) {
            continue;
        }
        let gain = increment * tuning.neighbor_factor * weight;
        // 同一跳内只取增益最大的一条路径，避免重复累加。
        match first_hop.get(&peer_id) {
            Some(existing) if *existing >= gain => {}
            _ => {
                first_hop.insert(peer_id, gain);
            }
        }
    }

    let mut propagated = 0usize;
    for (peer_id, gain) in &first_hop {
        if apply_activation(conn, peer_id, *gain, &half_life, &timestamp, session_id)? {
            propagated += 1;
        }
        seen.insert(peer_id.clone());
    }

    // 二跳及以后：增益按跳间衰减折算，只统计权重不低于阈值的活跃连线。
    let mut propagated_far = 0usize;
    let mut hops = 1usize;
    let mut frontier: Vec<(String, f64)> = first_hop.into_iter().collect();
    for _ in 2..=hop_count {
        let mut next: std::collections::BTreeMap<String, f64> =
            std::collections::BTreeMap::new();
        for (node_id, gain) in &frontier {
            for (edge_id, peer_id, weight) in neighbours_of(conn, node_id)? {
                bump_edge(conn, &edge_id, &timestamp)?;
                if weight < tuning.min_edge_weight || seen.contains(&peer_id) {
                    continue;
                }
                let candidate = gain * tuning.hop_decay;
                match next.get(&peer_id) {
                    Some(existing) if *existing >= candidate => {}
                    _ => {
                        next.insert(peer_id, candidate);
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        for (peer_id, gain) in &next {
            if apply_activation(conn, peer_id, *gain, &half_life, &timestamp, session_id)? {
                propagated_far += 1;
            }
            seen.insert(peer_id.clone());
        }
        hops += 1;
        frontier = next.into_iter().collect();
    }

    Ok(ActivationOutcome {
        activated,
        propagated,
        propagated_far,
        hops,
    })
}

/// 累计一次共激活并记录最近激活时刻。
fn bump_edge(conn: &Connection, edge_id: &str, timestamp: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE thought_edges
         SET co_activation_count = co_activation_count + 1, last_activated_at = ?2
         WHERE id = ?1",
        rusqlite::params![edge_id, timestamp],
    )?;
    Ok(())
}

fn neighbours_of(conn: &Connection, node_id: &str) -> CoreResult<Vec<(String, String, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT id, to_node_id, weight FROM thought_edges
         WHERE from_node_id = ?1 AND status = 'active'
         UNION ALL
         SELECT id, from_node_id, weight FROM thought_edges
         WHERE to_node_id = ?1 AND status = 'active'",
    )?;
    let rows = stmt.query_map([node_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 写入单点激活。返回是否命中节点。
fn apply_activation(
    conn: &Connection,
    node_id: &str,
    increment: f64,
    half_life: &f64,
    timestamp: &str,
    session_id: Option<&str>,
) -> CoreResult<bool> {
    let current: Option<(f64, String)> = conn
        .query_row(
            "SELECT activation, activation_updated_at FROM thought_nodes WHERE id = ?1",
            [node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((stored, updated_at)) = current else {
        return Ok(false);
    };

    let factor = decay_factor(elapsed_hours(conn, &updated_at)?, *half_life);
    let next = stored * factor + increment;
    conn.execute(
        "UPDATE thought_nodes SET activation = ?2, activation_updated_at = ?3 WHERE id = ?1",
        rusqlite::params![node_id, next, timestamp],
    )?;
    let id = unique_id("act", &format!("{node_id}:{timestamp}"));
    conn.execute(
        "INSERT INTO node_activations (id, node_id, session_id, increment, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, node_id, session_id, increment, timestamp],
    )?;
    Ok(true)
}

/// 按半衰期衰减全部存活节点，返回实际被改动的节点数。
pub fn decay_all(conn: &Connection, limit: i64) -> CoreResult<i64> {
    let half_life = half_life_hours(conn)?;
    let timestamp = now(conn)?;
    let limit = limit.clamp(1, super::MAX_CONSOLIDATION_BATCH);

    let mut stmt = conn.prepare(
        "SELECT id, activation, activation_updated_at FROM thought_nodes
         WHERE superseded_by IS NULL AND activation > 0
         ORDER BY activation_updated_at ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        candidates.push(row?);
    }
    drop(stmt);

    let mut changed = 0i64;
    for (id, stored, updated_at) in candidates {
        let factor = decay_factor(elapsed_hours(conn, &updated_at)?, half_life);
        if factor >= 1.0 {
            continue;
        }
        conn.execute(
            "UPDATE thought_nodes SET activation = ?2, activation_updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, stored * factor, timestamp],
        )?;
        changed += 1;
    }
    Ok(changed)
}

/// 读取节点详情：节点本体、直接连线与最近的激活记录。
pub fn get_node(conn: &Connection, node_id: &str) -> CoreResult<NodeDetail> {
    let node = load_node(conn, node_id)?;
    Ok(NodeDetail {
        links: links_of(conn, node_id)?,
        activations: activations_of(conn, node_id, 20)?,
        node,
    })
}

fn load_node(conn: &Connection, node_id: &str) -> CoreResult<ThoughtNode> {
    conn.query_row(
        "SELECT id, kind, content, normalized_content, source_kind, source_ref, domains_json,
                layers_json, activation, activation_updated_at, version, superseded_by,
                cluster_id, created_at
         FROM thought_nodes WHERE id = ?1",
        [node_id],
        map_node,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("网络节点 {node_id}")))
}

fn map_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThoughtNode> {
    Ok(ThoughtNode {
        id: row.get(0)?,
        kind: parse_kind(&row.get::<_, String>(1)?),
        content: row.get(2)?,
        normalized_content: row.get(3)?,
        source_kind: row.get(4)?,
        source_ref: row.get(5)?,
        domains: parse_strings(&row.get::<_, String>(6)?),
        layers: parse_layers(&row.get::<_, String>(7)?),
        activation: row.get(8)?,
        activation_updated_at: row.get(9)?,
        version: row.get(10)?,
        superseded_by: row.get(11)?,
        cluster_id: row.get(12)?,
        created_at: row.get(13)?,
    })
}

fn links_of(conn: &Connection, node_id: &str) -> CoreResult<Vec<NodeLink>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.relation, e.weight, e.status, e.from_node_id, e.to_node_id,
                n.kind, n.content
         FROM thought_edges e
         JOIN thought_nodes n
           ON n.id = CASE WHEN e.from_node_id = ?1 THEN e.to_node_id ELSE e.from_node_id END
         WHERE (e.from_node_id = ?1 OR e.to_node_id = ?1) AND e.status != 'dropped'
         ORDER BY e.weight DESC, e.id ASC",
    )?;
    let rows = stmt.query_map([node_id], |row| {
        let from: String = row.get(4)?;
        let to: String = row.get(5)?;
        let relation = parse_relation(&row.get::<_, String>(1)?);
        let direction = if relation.is_symmetric() {
            "both"
        } else if from == node_id {
            "out"
        } else {
            "in"
        };
        Ok(NodeLink {
            edge_id: row.get(0)?,
            relation,
            weight: row.get(2)?,
            status: row.get(3)?,
            direction: direction.to_string(),
            peer_id: if from == node_id { to } else { from },
            peer_kind: parse_kind(&row.get::<_, String>(6)?),
            peer_content: row.get(7)?,
        })
    })?;
    let mut links = Vec::new();
    for row in rows {
        links.push(row?);
    }
    Ok(links)
}

fn activations_of(conn: &Connection, node_id: &str, limit: i64) -> CoreResult<Vec<ActivationView>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, increment, occurred_at FROM node_activations
         WHERE node_id = ?1 ORDER BY occurred_at DESC, rowid DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![node_id, limit], |row| {
        Ok(ActivationView {
            id: row.get(0)?,
            session_id: row.get(1)?,
            increment: row.get(2)?,
            occurred_at: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 列出存活节点，按激活度倒序。
pub fn list_nodes(conn: &Connection, filter: &GraphFilter) -> CoreResult<Vec<ThoughtNode>> {
    let limit = filter
        .limit
        .unwrap_or(DEFAULT_GRAPH_LIMIT)
        .clamp(1, MAX_GRAPH_LIMIT);
    let mut stmt = conn.prepare(
        "SELECT id, kind, content, normalized_content, source_kind, source_ref, domains_json,
                layers_json, activation, activation_updated_at, version, superseded_by,
                cluster_id, created_at
         FROM thought_nodes
         WHERE superseded_by IS NULL
         ORDER BY activation DESC, created_at DESC, id ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], map_node)?;
    let mut nodes = Vec::new();
    for row in rows {
        nodes.push(row?);
    }
    Ok(nodes.into_iter().filter(|node| matches_filter(node, filter)).collect())
}

fn matches_filter(node: &ThoughtNode, filter: &GraphFilter) -> bool {
    if let Some(kind) = filter.kind {
        if node.kind != kind {
            return false;
        }
    }
    if let Some(min) = filter.min_activation {
        if node.activation < min {
            return false;
        }
    }
    if let Some(domain) = &filter.domain {
        if !node.domains.iter().any(|item| item == domain) {
            return false;
        }
    }
    if let Some(layer) = filter.layer {
        if !node.layers.contains(&layer) {
            return false;
        }
    }
    if let Some(cluster_id) = &filter.cluster_id {
        if node.cluster_id.as_deref() != Some(cluster_id.as_str()) {
            return false;
        }
    }
    true
}

/// 图谱快照：先取节点，再取两端都在集合内的存活连线。
pub fn get_graph(conn: &Connection, filter: &GraphFilter) -> CoreResult<GraphView> {
    let nodes = list_nodes(conn, filter)?;
    let ids: BTreeSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();

    let total_nodes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM thought_nodes WHERE superseded_by IS NULL",
        [],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, from_node_id, to_node_id, relation, weight FROM thought_edges
         WHERE status = 'active' ORDER BY weight DESC, id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(GraphEdge {
            id: row.get(0)?,
            from: row.get(1)?,
            to: row.get(2)?,
            relation: parse_relation(&row.get::<_, String>(3)?),
            weight: row.get(4)?,
        })
    })?;
    let mut edges = Vec::new();
    for row in rows {
        let edge = row?;
        if ids.contains(edge.from.as_str()) && ids.contains(edge.to.as_str()) {
            edges.push(edge);
        }
    }

    // 社区按可见节点归组，标签取 thought_clusters 中同名社区行。
    let mut grouped: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for node in &nodes {
        if let Some(cluster_id) = &node.cluster_id {
            grouped
                .entry(cluster_id.clone())
                .or_default()
                .push(node.id.clone());
        }
    }
    let mut clusters = Vec::new();
    for (cluster_id, member_ids) in grouped {
        let meta: Option<(String, String, String)> = conn
            .query_row(
                "SELECT label, domain, layer FROM thought_clusters WHERE id = ?1",
                [&cluster_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (label, domain, layer) = meta.unwrap_or_else(|| (cluster_id.clone(), String::new(), String::new()));
        clusters.push(ClusterView {
            member_count: member_ids.len() as i64,
            id: cluster_id,
            label,
            domain,
            layer,
            member_ids,
        });
    }

    Ok(GraphView {
        truncated: (nodes.len() as i64) < total_nodes,
        total_nodes,
        nodes: nodes
            .into_iter()
            .map(|node| GraphNode {
                id: node.id,
                kind: node.kind,
                content: node.content,
                domains: node.domains,
                layers: node.layers,
                activation: node.activation,
                activation_updated_at: node.activation_updated_at,
                cluster_id: node.cluster_id,
            })
            .collect(),
        clusters,
        edges,
    })
}

/// 裁决冲突连线。decision 取 keep 或 drop，留档到洞察表。
pub fn resolve_conflict(
    conn: &Connection,
    edge_id: &str,
    decision: &str,
    reason: &str,
) -> CoreResult<String> {
    let next_status = match decision.trim() {
        "keep" | "保留" => "kept",
        "drop" | "舍弃" => "dropped",
        other => {
            return Err(CoreError::InvalidInput(format!(
                "未知裁决：{other}，只支持 keep 或 drop"
            )))
        }
    };

    let edge: Option<(String, String, String)> = conn
        .query_row(
            "SELECT from_node_id, to_node_id, relation FROM thought_edges WHERE id = ?1",
            [edge_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((from, to, relation)) = edge else {
        return Err(CoreError::NotFound(format!("连线 {edge_id}")));
    };
    if parse_relation(&relation) != Relation::Conflicts {
        return Err(CoreError::InvalidInput("只有冲突连线需要裁决".into()));
    }

    conn.execute(
        "UPDATE thought_edges SET status = ?2 WHERE id = ?1",
        rusqlite::params![edge_id, next_status],
    )?;

    let timestamp = now(conn)?;
    let id = unique_id("insight", &format!("decision:{edge_id}:{next_status}"));
    let title = match next_status {
        "kept" => "冲突保留",
        _ => "冲突舍弃",
    };
    conn.execute(
        "INSERT INTO insights
             (id, kind, title, summary, related_node_ids_json, related_master_ids_json,
              evidence_json, status, action, reason, created_at)
         VALUES (?1, 'conflict_decision', ?2, ?3, ?4, '[]', '[]', 'closed', ?5, ?6, ?7)",
        rusqlite::params![
            id,
            title,
            format!("对连线 {edge_id} 的裁决已留档"),
            json_strings(&[from, to]),
            decision.trim(),
            reason,
            timestamp,
        ],
    )?;
    Ok(id)
}

/// 写入思考记录的输入。
pub struct NewRecord<'a> {
    pub session_id: Option<&'a str>,
    pub question: &'a str,
    pub domains: &'a [String],
    pub layers: &'a [Layer],
    pub conclusion: &'a str,
}

/// 写入一条思考记录，返回其 id 与主题键。
pub fn write_record(conn: &Connection, input: &NewRecord<'_>) -> CoreResult<(String, String)> {
    let question = input.question.trim();
    if question.is_empty() {
        return Err(CoreError::InvalidInput("思考记录的问题不能为空".into()));
    }
    let topic_key = normalize(question);
    let timestamp = now(conn)?;
    let id = unique_id("record", &format!("{topic_key}:{timestamp}"));
    conn.execute(
        "INSERT INTO thought_records
             (id, session_id, question, topic_key, domains_json, layers_json, conclusion, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            id,
            input.session_id,
            question,
            topic_key,
            json_strings(input.domains),
            layers_json(input.layers),
            input.conclusion,
            timestamp,
        ],
    )?;
    Ok((id, topic_key))
}

/// 某个会诊会话对应的思考记录，取最近写入的一条。
pub fn record_by_session(conn: &Connection, session_id: &str) -> CoreResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM thought_records WHERE session_id = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [session_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// 同一主题键下的历史判断节点，用于把新结论接到旧结论上。
pub fn prior_records(conn: &Connection, topic_key: &str, exclude_id: &str) -> CoreResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM thought_records
         WHERE topic_key = ?1 AND id != ?2 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![topic_key, exclude_id], |row| row.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 节点来源引用，供来源追溯。
pub fn source_of(conn: &Connection, node_id: &str) -> CoreResult<(String, String)> {
    let node = load_node(conn, node_id)?;
    Ok((node.source_kind, node.source_ref))
}

/// 按来源反查存活节点。同一来源重复写入时用于定位既有节点。
pub fn find_by_source(
    conn: &Connection,
    source_kind: &str,
    source_ref: &str,
) -> CoreResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM thought_nodes
             WHERE source_kind = ?1 AND source_ref = ?2 AND superseded_by IS NULL
             ORDER BY created_at ASC, rowid ASC LIMIT 1",
            rusqlite::params![source_kind, source_ref],
            |row| row.get(0),
        )
        .optional()?)
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThoughtRecordView> {
    Ok(ThoughtRecordView {
        id: row.get(0)?,
        session_id: row.get(1)?,
        question: row.get(2)?,
        topic_key: row.get(3)?,
        domains: parse_strings(&row.get::<_, String>(4)?),
        layers: parse_layers(&row.get::<_, String>(5)?),
        conclusion: row.get(6)?,
        adopted: row.get::<_, i64>(7)? != 0,
        reason: row.get(8)?,
        created_at: row.get(9)?,
    })
}

const RECORD_SELECT: &str = "SELECT id, session_id, question, topic_key, domains_json,
        layers_json, conclusion, adopted, reason, created_at FROM thought_records";

/// 按时间倒序读取思考记录。
pub fn list_records(conn: &Connection, limit: i64) -> CoreResult<Vec<ThoughtRecordView>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn.prepare(&format!(
        "{RECORD_SELECT} ORDER BY created_at DESC, rowid DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit], map_record)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

/// 同一主题下的全部记录，按时间正序，用于观察判断演化链。
pub fn compare_records(conn: &Connection, topic_key: &str) -> CoreResult<Vec<ThoughtRecordView>> {
    let mut stmt = conn.prepare(&format!(
        "{RECORD_SELECT} WHERE topic_key = ?1 ORDER BY created_at ASC, rowid ASC"
    ))?;
    let rows = stmt.query_map([topic_key], map_record)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

/// 标记某条思考记录是否被采纳，并记录理由。
pub fn mark_record_decision(
    conn: &Connection,
    record_id: &str,
    adopted: bool,
    reason: &str,
) -> CoreResult<()> {
    let affected = conn.execute(
        "UPDATE thought_records SET adopted = ?2, reason = ?3 WHERE id = ?1",
        rusqlite::params![record_id, i64::from(adopted), reason],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("思考记录 {record_id}")));
    }
    Ok(())
}
