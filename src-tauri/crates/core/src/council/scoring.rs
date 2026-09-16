//! 选角评分：相关度、对立度与领域距离。
//!
//! 三项评分全部由本地数据确定性推导，不依赖模型调用，因此同一话题与同一
//! 大师库下选角结果可复现。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rusqlite::Connection;

use crate::error::CoreResult;
use crate::master::Layer;

/// 领域之间不可达时使用的距离，代表与话题领域最远。
pub const MAX_DOMAIN_DISTANCE: f64 = 10.0;

fn is_cjk(ch: char) -> bool {
    ('\u{3400}'..='\u{9fff}').contains(&ch)
}

/// 中文取相邻二字，英文取整词。分词只用于比较，不做语言学处理。
pub fn tokens(text: &str) -> BTreeSet<String> {
    let normalized = text.to_lowercase();
    let mut out = BTreeSet::new();
    let mut cjk_run: Vec<char> = Vec::new();
    let mut word = String::new();

    let flush_word = |word: &mut String, out: &mut BTreeSet<String>| {
        if word.chars().count() >= 2 {
            out.insert(word.clone());
        }
        word.clear();
    };
    let flush_cjk = |run: &mut Vec<char>, out: &mut BTreeSet<String>| {
        for pair in run.windows(2) {
            out.insert(pair.iter().collect::<String>());
        }
        run.clear();
    };

    for ch in normalized.chars() {
        if is_cjk(ch) {
            flush_word(&mut word, &mut out);
            cjk_run.push(ch);
        } else if ch.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk_run, &mut out);
            word.push(ch);
        } else {
            flush_word(&mut word, &mut out);
            flush_cjk(&mut cjk_run, &mut out);
        }
    }
    flush_word(&mut word, &mut out);
    flush_cjk(&mut cjk_run, &mut out);
    out
}

/// 重叠系数：交集除以较小集合大小，比 Jaccard 对短文本更敏感。
pub fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    inter / (a.len().min(b.len()) as f64)
}

/// 相关度：话题分词被大师文本覆盖的比例。
pub fn relevance(topic: &BTreeSet<String>, master: &BTreeSet<String>) -> f64 {
    if topic.is_empty() || master.is_empty() {
        return 0.0;
    }
    let inter = topic.intersection(master).count() as f64;
    inter / topic.len() as f64
}

/// 对立度：用词差异为主，层次差异为辅。
pub fn opposition(
    a_tokens: &BTreeSet<String>,
    a_layers: &[Layer],
    b_tokens: &BTreeSet<String>,
    b_layers: &[Layer],
) -> f64 {
    let vocab = 1.0 - overlap(a_tokens, b_tokens);
    let a_set: BTreeSet<u8> = a_layers.iter().map(|layer| *layer as u8).collect();
    let b_set: BTreeSet<u8> = b_layers.iter().map(|layer| *layer as u8).collect();
    let union = a_set.union(&b_set).count() as f64;
    let inter = a_set.intersection(&b_set).count() as f64;
    let layer_gap = if union == 0.0 { 0.0 } else { 1.0 - inter / union };
    0.6 * vocab + 0.4 * layer_gap
}

/// 领域邻接图：共享至少一个层次的两个领域视为相邻。
pub fn domain_graph(conn: &Connection) -> CoreResult<BTreeMap<String, BTreeSet<String>>> {
    let mut stmt = conn.prepare("SELECT domain, layers_json FROM masters")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut by_layer: BTreeMap<u8, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (domain, layers_json) = row?;
        let names: Vec<String> = serde_json::from_str(&layers_json).unwrap_or_default();
        for name in names {
            if let Some(layer) = Layer::parse(&name) {
                by_layer.entry(layer as u8).or_default().insert(domain.clone());
            }
        }
    }

    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for domains in by_layer.values() {
        for left in domains {
            for right in domains {
                if left != right {
                    graph.entry(left.clone()).or_default().insert(right.clone());
                }
            }
        }
    }
    Ok(graph)
}

/// 多源 BFS：返回某个领域到最近话题领域的最短距离，不可达按最远计。
pub fn domain_distance(
    graph: &BTreeMap<String, BTreeSet<String>>,
    topic_domains: &[String],
    target: &str,
) -> f64 {
    if topic_domains.iter().any(|domain| domain == target) {
        return 0.0;
    }
    if topic_domains.is_empty() {
        return 0.0;
    }

    let mut seen: BTreeSet<String> = topic_domains.iter().cloned().collect();
    let mut queue: VecDeque<(String, i64)> = topic_domains
        .iter()
        .map(|domain| (domain.clone(), 0))
        .collect();

    while let Some((current, depth)) = queue.pop_front() {
        if current == target {
            return depth as f64;
        }
        for next in graph.get(&current).into_iter().flatten() {
            if seen.insert(next.clone()) {
                queue.push_back((next.clone(), depth + 1));
            }
        }
    }
    MAX_DOMAIN_DISTANCE
}
