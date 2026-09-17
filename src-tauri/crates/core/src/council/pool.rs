//! 候选池：把大师库整理成带三项评分的候选集合。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::error::CoreResult;
use crate::master::{Layer, LAYER_ORDER};

use super::pairings;
use super::scoring;
use super::{Candidate, CandidatePool};

/// 大师的文本指纹，用于相关度与对立度。
pub struct MasterText {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub layers: Vec<Layer>,
    pub tokens: BTreeSet<String>,
    /// 按题（层次）分组的单元文本指纹，供同题对立度使用。
    pub layer_tokens: BTreeMap<Layer, BTreeSet<String>>,
}

/// 汇总大师身份与当前版本技能单元文本。
pub fn load_master_texts(conn: &Connection) -> CoreResult<Vec<MasterText>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.name, m.domain, m.layers_json, m.summary, m.style,
                (SELECT GROUP_CONCAT(u.title || ' ' || u.mechanism || ' ' || u.boundary, ' ')
                   FROM master_units u
                  WHERE u.master_id = m.id AND u.version = m.current_version)
         FROM masters m
         ORDER BY m.id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let mut masters = Vec::new();
    for row in rows {
        let (id, name, domain, layers_json, summary, style, units) = row?;
        let names: Vec<String> = serde_json::from_str(&layers_json).unwrap_or_default();
        let mut layers: Vec<Layer> = names.iter().filter_map(|name| Layer::parse(name)).collect();
        layers.sort();
        let text = format!(
            "{name} {domain} {summary} {style} {units}",
            units = units.unwrap_or_default()
        );
        masters.push(MasterText {
            id,
            name,
            domain,
            layers,
            tokens: scoring::tokens(&text),
            layer_tokens: BTreeMap::new(),
        });
    }

    // 单元按题分组：同一题下只比较这一题的用词，跨题内容不参与。
    let mut stmt = conn.prepare(
        "SELECT u.master_id, u.layer, u.title || ' ' || u.mechanism || ' ' || u.boundary
           FROM master_units u
           JOIN masters m ON m.id = u.master_id
          WHERE u.version = m.current_version
          ORDER BY u.master_id ASC, u.ordinal ASC",
    )?;
    let unit_rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut by_master: BTreeMap<String, BTreeMap<Layer, String>> = BTreeMap::new();
    for row in unit_rows {
        let (master_id, layer_name, text) = row?;
        let Some(layer) = Layer::parse(&layer_name) else {
            continue;
        };
        let entry = by_master.entry(master_id).or_default().entry(layer).or_default();
        entry.push(' ');
        entry.push_str(&text);
    }
    for master in &mut masters {
        if let Some(units) = by_master.remove(&master.id) {
            master.layer_tokens = units
                .into_iter()
                .map(|(layer, text)| (layer, scoring::tokens(&text)))
                .collect();
        }
    }
    Ok(masters)
}

/// 话题输入。
pub struct TopicInput<'a> {
    pub question: &'a str,
    /// 用户显式指定的领域，为空时从问题文本中识别。
    pub domains: &'a [String],
}

/// 从问题与标签文本中识别已知领域。
pub fn detect_domains(text: &str, masters: &[MasterText]) -> Vec<String> {
    let lowered = text.to_lowercase();
    let mut found: Vec<String> = masters
        .iter()
        .map(|master| master.domain.clone())
        .filter(|domain| !domain.is_empty() && lowered.contains(&domain.to_lowercase()))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// 构建候选池。三项评分全部由本地数据推导。
pub fn build(conn: &Connection, topic: &TopicInput<'_>) -> CoreResult<CandidatePool> {
    let masters = load_master_texts(conn)?;
    let pair_map = pairings::load(conn)?;
    let topic_tokens = scoring::tokens(topic.question);

    let mut domains: Vec<String> = topic.domains.to_vec();
    if domains.is_empty() {
        domains = detect_domains(topic.question, &masters);
    }

    // 领域距离的参照：优先用显式或识别出的领域，否则以相关度最高者所属领域为锚。
    let graph = scoring::domain_graph(conn)?;
    let mut anchor_domains = domains.clone();
    if anchor_domains.is_empty() {
        if let Some(best) = masters
            .iter()
            .max_by(|left, right| {
                let left_score = scoring::relevance(&topic_tokens, &left.tokens);
                let right_score = scoring::relevance(&topic_tokens, &right.tokens);
                left_score
                    .partial_cmp(&right_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| right.id.cmp(&left.id))
            })
        {
            if !best.domain.is_empty() {
                anchor_domains.push(best.domain.clone());
            }
        }
    }

    let mut candidates = Vec::new();
    for master in &masters {
        let relevance = scoring::relevance(&topic_tokens, &master.tokens);
        let opposition = masters
            .iter()
            .filter(|other| other.id != master.id)
            .map(|other| pairings::mutual(&pair_map, &master.id, &other.id))
            .fold(0.0_f64, f64::max);
        let domain_distance =
            scoring::domain_distance(&graph, &anchor_domains, &master.domain);
        candidates.push(Candidate {
            master_id: master.id.clone(),
            name: master.name.clone(),
            domain: master.domain.clone(),
            layers: master.layers.clone(),
            relevance,
            opposition,
            domain_distance,
        });
    }
    candidates.sort_by(|left, right| left.master_id.cmp(&right.master_id));

    let covered: BTreeSet<Layer> = candidates
        .iter()
        .flat_map(|candidate| candidate.layers.iter().copied())
        .collect();
    let missing_layers: Vec<Layer> = LAYER_ORDER
        .iter()
        .copied()
        .filter(|layer| !covered.contains(layer))
        .collect();

    let mut topic_token_list: Vec<String> = topic_tokens.into_iter().collect();
    topic_token_list.sort();

    Ok(CandidatePool {
        question: topic.question.to_string(),
        domains,
        topic_tokens: topic_token_list,
        candidates,
        missing_layers,
    })
}
