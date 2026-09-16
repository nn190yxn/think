//! 轮换策略选角。
//!
//! 选角完全确定性：候选池与评分不变时，同一策略对同一话题选出同一组大师。
//! 层次覆盖作为硬约束优先满足，随后才按策略补齐剩余席位。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::error::CoreResult;
use crate::master::{Layer, LAYER_ORDER};

use super::pairings;
use super::{
    Candidate, CandidatePool, Selection, Strategy, DEFAULT_PANEL_SIZE, MAX_PANEL_SIZE,
    MIN_PANEL_SIZE,
};

/// 一次选角的输入。
pub struct SelectionRequest<'a> {
    pub strategy: Strategy,
    /// 期望席位数，为 0 时取默认六席。
    pub size: usize,
    /// 用户保留席位，必须入席。
    pub pinned: &'a [String],
    /// 换批时排除的大师，保留席位不受此影响。
    pub exclude: &'a [String],
}

impl<'a> SelectionRequest<'a> {
    pub fn new(strategy: Strategy) -> Self {
        Self {
            strategy,
            size: DEFAULT_PANEL_SIZE,
            pinned: &[],
            exclude: &[],
        }
    }
}

/// 席位主要代表的层次：取该大师声明层次中最靠抽象端的一层。
fn seat_layer(candidate: &Candidate) -> Layer {
    LAYER_ORDER
        .iter()
        .copied()
        .find(|layer| candidate.layers.contains(layer))
        .or_else(|| candidate.layers.first().copied())
        .unwrap_or(Layer::Fa)
}

/// 碰撞策略下，候选与已入席者之间的对立度之和。
fn opposition_to_selected(
    pair_map: &BTreeMap<(String, String), f64>,
    candidate: &Candidate,
    selected: &[String],
) -> f64 {
    selected
        .iter()
        .map(|id| pairings::mutual(pair_map, &candidate.master_id, id))
        .sum()
}

/// 策略下的候选排序分。碰撞策略在已有席位时改用与席位的对立度。
fn rank_score(
    strategy: Strategy,
    pair_map: &BTreeMap<(String, String), f64>,
    candidate: &Candidate,
    selected: &[String],
) -> f64 {
    match strategy {
        Strategy::Steady => candidate.relevance,
        Strategy::Serendipity => candidate.domain_distance,
        Strategy::Clash => {
            if selected.is_empty() {
                candidate.opposition
            } else {
                opposition_to_selected(pair_map, candidate, selected)
            }
        }
    }
}

/// 按策略从候选池中选出入席阵容。
pub fn select_panel(
    conn: &Connection,
    pool: &CandidatePool,
    request: &SelectionRequest<'_>,
) -> CoreResult<Selection> {
    let pair_map = pairings::load(conn)?;
    let size = if request.size == 0 {
        DEFAULT_PANEL_SIZE
    } else {
        request.size.clamp(MIN_PANEL_SIZE, MAX_PANEL_SIZE)
    };

    let by_id: BTreeMap<&str, &Candidate> = pool
        .candidates
        .iter()
        .map(|candidate| (candidate.master_id.as_str(), candidate))
        .collect();
    let excluded: BTreeSet<&str> = request.exclude.iter().map(String::as_str).collect();

    let mut seats = Vec::new();
    let mut selected: Vec<String> = Vec::new();
    let mut selected_ids: BTreeSet<String> = BTreeSet::new();
    // 本阵容实际覆盖到的层次。席位层次取「本次补位层次」，而非大师的主层次，
    // 否则多层大师会被重复算作同一层，覆盖约束形同虚设。
    let mut covered: BTreeSet<Layer> = BTreeSet::new();

    // 保留席位优先入席，不受换批排除影响。
    for id in request.pinned {
        if seats.len() >= size {
            break;
        }
        if let Some(candidate) = by_id.get(id.as_str()) {
            if selected_ids.insert(candidate.master_id.clone()) {
                selected.push(candidate.master_id.clone());
                let layer = seat_layer(candidate);
                covered.insert(layer);
                seats.push(make_seat(
                    candidate,
                    layer,
                    request.strategy,
                    &pair_map,
                    &[],
                    true,
                ));
            }
        }
    }

    // 层次覆盖优先：把「未覆盖层次」与「可用候选」看成二部图，取最大匹配。
    // 只用贪心会让独苗大师被其他层先抢走，从而在别处留下本可避免的空缺。
    let available: Vec<&Candidate> = pool
        .candidates
        .iter()
        .filter(|candidate| !selected_ids.contains(&candidate.master_id))
        .collect();

    let targets: Vec<Layer> = LAYER_ORDER
        .iter()
        .copied()
        .filter(|layer| !covered.contains(layer))
        .collect();

    // 每层按策略偏好给候选排序，作为匹配时的优先顺序。
    let adjacency: Vec<Vec<usize>> = targets
        .iter()
        .map(|layer| {
            let mut indexes: Vec<usize> = (0..available.len())
                .filter(|index| available[*index].layers.contains(layer))
                .collect();
            indexes.sort_by(|left, right| {
                compare(
                    &pair_map,
                    &selected,
                    request.strategy,
                    &excluded,
                    available[*left],
                    available[*right],
                )
            });
            indexes
        })
        .collect();

    let mut layer_of_candidate: Vec<Option<usize>> = vec![None; available.len()];
    let mut candidate_of_layer: Vec<Option<usize>> = vec![None; targets.len()];

    // 先安排候选最少的层次，提升匹配成功率。
    let mut order: Vec<usize> = (0..targets.len()).collect();
    order.sort_by_key(|index| (adjacency[*index].len(), *index));
    for index in order {
        let mut visited = vec![false; available.len()];
        augment(
            index,
            &adjacency,
            &mut visited,
            &mut layer_of_candidate,
            &mut candidate_of_layer,
        );
    }

    for (layer_index, layer) in targets.iter().enumerate() {
        if seats.len() >= size {
            break;
        }
        if let Some(candidate_index) = candidate_of_layer[layer_index] {
            let candidate = available[candidate_index];
            selected_ids.insert(candidate.master_id.clone());
            selected.push(candidate.master_id.clone());
            covered.insert(*layer);
            seats.push(make_seat(
                candidate,
                *layer,
                request.strategy,
                &pair_map,
                &selected[..selected.len() - 1],
                false,
            ));
        }
    }

    // 剩余席位按策略全局补齐。
    while seats.len() < size {
        let best = pool
            .candidates
            .iter()
            .filter(|candidate| !selected_ids.contains(&candidate.master_id))
            .min_by(|left, right| {
                compare(
                    &pair_map,
                    &selected,
                    request.strategy,
                    &excluded,
                    left,
                    right,
                )
            });
        match best {
            Some(candidate) => {
                selected_ids.insert(candidate.master_id.clone());
                selected.push(candidate.master_id.clone());
                let layer = seat_layer(candidate);
                covered.insert(layer);
                seats.push(make_seat(
                    candidate,
                    layer,
                    request.strategy,
                    &pair_map,
                    &selected[..selected.len() - 1],
                    false,
                ));
            }
            None => break,
        }
    }

    let layers: Vec<Layer> = covered.iter().copied().collect();

    let gaps: Vec<Layer> = LAYER_ORDER
        .iter()
        .copied()
        .filter(|layer| !layers.contains(layer))
        .collect();

    Ok(Selection {
        strategy: request.strategy,
        size,
        seats,
        layers,
        gaps,
    })
}

/// 换批排除项不是硬过滤，而是重罚：候选池被抽干时仍能回头选中老人，
/// 避免为了「换人」把层次覆盖约束打穿。
const EXCLUSION_PENALTY: f64 = 1_000_000.0;

/// 排序比较：分数高者优先，同分按 id 升序，保证确定性。
fn compare(
    pair_map: &BTreeMap<(String, String), f64>,
    selected: &[String],
    strategy: Strategy,
    excluded: &BTreeSet<&str>,
    left: &Candidate,
    right: &Candidate,
) -> std::cmp::Ordering {
    let penalty = |candidate: &Candidate| {
        if excluded.contains(candidate.master_id.as_str()) {
            EXCLUSION_PENALTY
        } else {
            0.0
        }
    };
    let left_score = rank_score(strategy, pair_map, left, selected) - penalty(left);
    let right_score = rank_score(strategy, pair_map, right, selected) - penalty(right);
    right_score
        .partial_cmp(&left_score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| left.master_id.cmp(&right.master_id))
}

/// Kuhn 增广：为某一层找一个候选，必要时让已分配的层改换候选。
fn augment(
    layer_index: usize,
    adjacency: &[Vec<usize>],
    visited: &mut [bool],
    layer_of_candidate: &mut [Option<usize>],
    candidate_of_layer: &mut [Option<usize>],
) -> bool {
    for candidate in &adjacency[layer_index] {
        if visited[*candidate] {
            continue;
        }
        visited[*candidate] = true;
        let reassignable = match layer_of_candidate[*candidate] {
            None => true,
            Some(other) => augment(
                other,
                adjacency,
                visited,
                layer_of_candidate,
                candidate_of_layer,
            ),
        };
        if reassignable {
            layer_of_candidate[*candidate] = Some(layer_index);
            candidate_of_layer[layer_index] = Some(*candidate);
            return true;
        }
    }
    false
}

fn make_seat(
    candidate: &Candidate,
    layer: Layer,
    strategy: Strategy,
    pair_map: &BTreeMap<(String, String), f64>,
    selected: &[String],
    pinned: bool,
) -> super::Seat {
    super::Seat {
        master_id: candidate.master_id.clone(),
        name: candidate.name.clone(),
        domain: candidate.domain.clone(),
        layers: candidate.layers.clone(),
        layer,
        score: rank_score(strategy, pair_map, candidate, selected),
        pinned,
    }
}
