//! 后台轻量碰撞：为新信号检索相关节点与大师框架，做一次模型调用并产出洞察。

use std::collections::BTreeSet;

use rusqlite::Connection;
use serde::Deserialize;

use crate::error::CoreResult;
use crate::llm::{call_model, ModelClient, ModelRequest, RetryPolicy};
use crate::master::{repo as master_repo, Layer};
use crate::network::repo as network_repo;
use crate::network::repo::MAX_GRAPH_LIMIT;
use crate::network::{GraphFilter, NodeKind};

use super::repo;
use super::{
    CollisionOutcome, CollisionSignal, CompanionRules, InsightKind, NewInsight, MAX_CONTEXT_CHARS,
    MAX_CONTEXT_NODES, SOURCE_COMPANION,
};

/// 写入调用审计的用途标记。
pub const COLLIDE_PURPOSE: &str = "companion_collide";
/// 单次碰撞最多参考的大师数。
const MAX_CONTEXT_MASTERS: usize = 3;

/// 检索到的上下文节点。
#[derive(Debug, Clone)]
pub struct ContextNode {
    pub id: String,
    pub kind: NodeKind,
    pub content: String,
}

/// 模型返回的候选洞察。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParsedInsight {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub node_ids: Vec<String>,
    pub master_ids: Vec<String>,
}

/// 内容按字符二元组切分，用于轻量的相关性打分。
fn bigrams(text: &str) -> BTreeSet<String> {
    let normalized = network_repo::normalize(text);
    let chars: Vec<char> = normalized.chars().collect();
    if chars.len() < 2 {
        return chars.iter().map(|c| c.to_string()).collect();
    }
    chars
        .windows(2)
        .map(|pair| pair.iter().collect::<String>())
        .collect()
}

fn overlap(signal: &BTreeSet<String>, candidate: &BTreeSet<String>) -> usize {
    signal.intersection(candidate).count()
}

/// 在图谱中检索与信号相关的节点，按相关度与激活度排序后截断。
pub fn retrieve_nodes(
    conn: &Connection,
    signal: &CollisionSignal,
    rules: &CompanionRules,
) -> CoreResult<Vec<ContextNode>> {
    let limit = rules.context_nodes.clamp(1, MAX_CONTEXT_NODES);
    let filter = GraphFilter {
        limit: Some(MAX_GRAPH_LIMIT),
        ..GraphFilter::default()
    };
    let nodes = network_repo::list_nodes(conn, &filter)?;
    let signal_grams = bigrams(&signal.content);

    let mut scored: Vec<(f64, f64, String, ContextNode)> = Vec::new();
    for node in nodes {
        if node.activation < rules.min_activation {
            continue;
        }
        let domain_hit = node
            .domains
            .iter()
            .any(|domain| signal.domains.contains(domain));
        let layer_hit = node
            .layers
            .iter()
            .any(|layer| signal.layers.contains(layer));
        let gram_hit = overlap(&signal_grams, &bigrams(&node.content));
        let score = gram_hit as f64
            + if domain_hit { 2.0 } else { 0.0 }
            + if layer_hit { 1.0 } else { 0.0 };
        if score <= 0.0 {
            continue;
        }
        scored.push((
            score,
            node.activation,
            node.id.clone(),
            ContextNode {
                id: node.id,
                kind: node.kind,
                content: node.content,
            },
        ));
    }

    // 相关度优先，其次激活度，最后用 id 保证顺序稳定。
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then_with(|| a.2.cmp(&b.2))
    });

    let mut picked = Vec::new();
    let mut budget = MAX_CONTEXT_CHARS;
    for (_, _, _, node) in scored {
        if picked.len() as i64 >= limit {
            break;
        }
        let cost = node.content.chars().count() + node.kind.name().chars().count() + 8;
        if cost > budget && !picked.is_empty() {
            break;
        }
        budget = budget.saturating_sub(cost);
        picked.push(node);
    }
    Ok(picked)
}

/// 按信号领域检索可参考的大师框架，返回 (id, name)。
pub fn retrieve_masters(
    conn: &Connection,
    signal: &CollisionSignal,
) -> CoreResult<Vec<(String, String)>> {
    let mut seen = BTreeSet::new();
    let mut picked = Vec::new();
    for domain in &signal.domains {
        if picked.len() >= MAX_CONTEXT_MASTERS {
            break;
        }
        for master in master_repo::list(conn, Some(domain), None)? {
            if picked.len() >= MAX_CONTEXT_MASTERS {
                break;
            }
            if seen.insert(master.id.clone()) {
                picked.push((master.id, master.name));
            }
        }
    }
    Ok(picked)
}

fn build_prompt(
    signal: &CollisionSignal,
    nodes: &[ContextNode],
    masters: &[(String, String)],
) -> (String, String) {
    let system = "你是思想熔炉的主动助理。只在发现真实的关联、冲突或盲区时才产出洞察；\
没有发现就返回空数组。严格只输出 JSON 数组，不要任何解释或代码块标记。每个元素形如：\
{\"kind\":\"relation|conflict|blindspot\",\"title\":\"一句话标题\",\"summary\":\"两句以内说明\",\
\"nodeIds\":[\"相关节点id\"],\"masterIds\":[\"相关大师id\"]}。"
        .to_string();

    let mut user = format!(
        "新信号（来源：{}）：\n{}\n\n",
        signal.source_kind,
        signal.content.trim()
    );
    if nodes.is_empty() {
        user.push_str("网络中尚未找到与之相连的认知节点。\n");
    } else {
        user.push_str("网络中相关的认知节点：\n");
        for node in nodes {
            user.push_str(&format!(
                "- [{}] {}：{}\n",
                node.id,
                node.kind.name(),
                node.content
            ));
        }
    }
    if !masters.is_empty() {
        user.push_str("\n可参考的大师框架：\n");
        for (id, name) in masters {
            user.push_str(&format!("- [{id}] {name}\n"));
        }
    }
    user.push_str("\n请基于以上材料产出洞察。");
    (system, user)
}

/// 解析模型返回的洞察数组，容忍代码块围栏与夹带说明。
pub fn parse_insights(raw: &str) -> Vec<ParsedInsight> {
    let trimmed = raw.trim();
    let body = match (trimmed.find('['), trimmed.rfind(']')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => return Vec::new(),
    };
    serde_json::from_str::<Vec<ParsedInsight>>(body).unwrap_or_default()
}

/// 依据来源与规则判断本次信号是否应触发碰撞。
fn rule_skip(source_kind: &str, rules: &CompanionRules) -> Option<String> {
    match source_kind.trim().to_ascii_lowercase().as_str() {
        "capture" | "capture_event" | "采集" => {
            if rules.trigger_on_capture {
                None
            } else {
                Some("rule_capture_off".to_string())
            }
        }
        _ => {
            if rules.trigger_on_record {
                None
            } else {
                Some("rule_record_off".to_string())
            }
        }
    }
}

fn truncated(value: &str, limit: usize) -> String {
    let chars: Vec<char> = value.trim().chars().collect();
    if chars.len() <= limit {
        return chars.into_iter().collect();
    }
    let mut text: String = chars[..limit].iter().collect();
    text.push('…');
    text
}

fn skipped(reason: &str, remaining: i64) -> CollisionOutcome {
    CollisionOutcome {
        generated: Vec::new(),
        pushed: 0,
        skipped: Some(reason.to_string()),
        remaining,
    }
}

/// 对一条信号做一次轻量碰撞。
///
/// 关闭、规则未命中或超出当日上限时直接跳过，不发起模型调用。模型调用失败
/// 等同于一次跳过：只记入调用审计，不打扰用户。
pub fn collide(
    conn: &Connection,
    client: &dyn ModelClient,
    signal: &CollisionSignal,
    policy: &RetryPolicy,
) -> CoreResult<CollisionOutcome> {
    let settings = repo::get_settings(conn)?;
    let remaining = repo::remaining_today(conn, &settings)?;
    if !settings.enabled {
        return Ok(skipped("disabled", remaining));
    }
    if let Some(reason) = rule_skip(&signal.source_kind, &settings.rules) {
        return Ok(skipped(&reason, remaining));
    }
    if remaining <= 0 {
        return Ok(skipped("limit", remaining));
    }

    let nodes = retrieve_nodes(conn, signal, &settings.rules)?;
    let masters = retrieve_masters(conn, signal)?;
    let master_ids: Vec<String> = masters.iter().map(|(id, _)| id.clone()).collect();
    let (system, user) = build_prompt(signal, &nodes, &masters);
    let request = ModelRequest::new(COLLIDE_PURPOSE, system, user);

    let response = match call_model(conn, client, &request, policy) {
        Ok(response) => response,
        Err(error) => return Ok(skipped(error.code(), remaining)),
    };

    let mut budget = remaining;
    let mut generated = Vec::new();
    let evidence = vec![signal.source_ref.clone()];
    for candidate in parse_insights(&response.content) {
        if budget <= 0 {
            break;
        }
        let Some(kind) = InsightKind::parse(&candidate.kind) else {
            continue;
        };
        if candidate.title.trim().is_empty() {
            continue;
        }
        let input = NewInsight {
            kind,
            title: &candidate.title,
            summary: &candidate.summary,
            related_node_ids: &candidate.node_ids,
            related_master_ids: &candidate.master_ids,
            evidence: &evidence,
            source: SOURCE_COMPANION,
        };
        if let Some(insight) = repo::push_insight(conn, &settings, &input)? {
            generated.push(insight);
            budget -= 1;
        } else {
            break;
        }
    }

    // 图谱中没有任何相关节点，说明这是一片尚未连接的认知盲区。
    let has_blindspot = generated
        .iter()
        .any(|insight| insight.kind == InsightKind::Blindspot);
    if nodes.is_empty() && !has_blindspot && budget > 0 {
        let summary = format!(
            "「{}」尚未与网络中的任何判断或框架相连，可以先记下，再慢慢补上连接。",
            truncated(&signal.content, 40)
        );
        let input = NewInsight {
            kind: InsightKind::Blindspot,
            title: "认知盲区",
            summary: &summary,
            related_node_ids: &[],
            related_master_ids: &master_ids,
            evidence: &evidence,
            source: SOURCE_COMPANION,
        };
        if let Some(insight) = repo::push_insight(conn, &settings, &input)? {
            generated.push(insight);
            budget -= 1;
        }
    }

    let pushed = generated.len() as i64;
    Ok(CollisionOutcome {
        generated,
        pushed,
        skipped: None,
        remaining: budget,
    })
}

/// 供测试与外部调用合并使用的默认重试策略。
pub fn default_policy() -> RetryPolicy {
    RetryPolicy::default()
}

/// 便于调用方复用：解析信号中的层次标签。
pub fn parse_layers(values: &[String]) -> Vec<Layer> {
    values.iter().filter_map(|name| Layer::parse(name)).collect()
}
