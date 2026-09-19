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
    Candidate, CandidatePool, SeatRef, Selection, Strategy, DEFAULT_PANEL_SIZE, MAX_PANEL_SIZE,
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
    /// 上一轮阵容的席位指派。换批时用它知道每一题上一任是谁，优先换入立场不同的人。
    pub previous: &'a [SeatRef],
    /// 上一轮没能谈拢的题。这些题即使有人站上也仍记为缺口，换批时优先换人再谈。
    pub diverged: &'a [Layer],
}

impl<'a> SelectionRequest<'a> {
    pub fn new(strategy: Strategy) -> Self {
        Self {
            strategy,
            size: DEFAULT_PANEL_SIZE,
            pinned: &[],
            exclude: &[],
            previous: &[],
            diverged: &[],
        }
    }
}

/// 排序所需的全部上下文：策略、对立度表与已入席者。
struct RankContext<'a> {
    strategy: Strategy,
    pair_map: &'a BTreeMap<(String, String), f64>,
    layer_map: &'a BTreeMap<(String, String), BTreeMap<Layer, f64>>,
    previous: &'a BTreeMap<Layer, String>,
    selected: &'a [String],
}

/// 碰撞策略下，候选与已入席者之间的整体对立度之和。
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

/// 某题的现任发言者与该候选的同题对立度。
fn layer_opposition_to_previous(
    ctx: &RankContext<'_>,
    candidate: &Candidate,
    layer: Layer,
) -> Option<f64> {
    let previous = ctx.previous.get(&layer)?;
    pairings::mutual_layer(ctx.layer_map, &candidate.master_id, previous, layer)
}

/// 策略下的候选排序分。
///
/// 碰撞策略在补某一题时，优先选在该题上与上一任立场不同的人；
/// 没有上一任可参照时，退回与已入席者的整体对立度。
fn rank_score(ctx: &RankContext<'_>, candidate: &Candidate, layer: Option<Layer>) -> f64 {
    match ctx.strategy {
        Strategy::Steady => candidate.relevance,
        Strategy::Serendipity => candidate.domain_distance,
        Strategy::Clash => layer
            .and_then(|layer| layer_opposition_to_previous(ctx, candidate, layer))
            .unwrap_or_else(|| {
                if ctx.selected.is_empty() {
                    candidate.opposition
                } else {
                    opposition_to_selected(ctx.pair_map, candidate, ctx.selected)
                }
            }),
    }
}

/// 落座层：优先他有料、且本阵容尚未覆盖的题，取积累最深的一题；有料的题都已被
/// 覆盖时退回他最深的一题；完全没有单元时退回声明层。并列时按六题顺序取靠前的。
fn deepest_layer(candidate: &Candidate, covered: &BTreeSet<Layer>) -> Layer {
    for pass in 0..2 {
        let mut best: Option<(Layer, usize)> = None;
        for layer in LAYER_ORDER {
            if pass == 0 && covered.contains(&layer) {
                continue;
            }
            let count = candidate.layer_depth.get(&layer).copied().unwrap_or(0);
            if count == 0 {
                continue;
            }
            if best.map_or(true, |(_, best_count)| count > best_count) {
                best = Some((layer, count));
            }
        }
        if let Some((layer, _)) = best {
            return layer;
        }
    }
    super::primary_layer(&candidate.layers)
}

/// 按策略从候选池中选出入席阵容。
pub fn select_panel(
    conn: &Connection,
    pool: &CandidatePool,
    request: &SelectionRequest<'_>,
) -> CoreResult<Selection> {
    let pair_map = pairings::load(conn)?;
    let layer_map = pairings::load_by_layer(conn)?;
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
    // 上一轮每题的发言者，供碰撞策略在补该题时找立场不同的人。
    let previous: BTreeMap<Layer, String> = request
        .previous
        .iter()
        .map(|seat| (seat.layer, seat.master_id.clone()))
        .collect();

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
                let layer = deepest_layer(candidate, &covered);
                covered.insert(layer);
                seats.push(make_seat(
                    &RankContext {
                        strategy: request.strategy,
                        pair_map: &pair_map,
                        layer_map: &layer_map,
                        previous: &previous,
                        selected: &selected[..selected.len() - 1],
                    },
                    candidate,
                    layer,
                    Some(layer),
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
                    &RankContext {
                        strategy: request.strategy,
                        pair_map: &pair_map,
                        layer_map: &layer_map,
                        previous: &previous,
                        selected: &selected,
                    },
                    &excluded,
                    available[*left],
                    available[*right],
                    Some(*layer),
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
                &RankContext {
                    strategy: request.strategy,
                    pair_map: &pair_map,
                    layer_map: &layer_map,
                    previous: &previous,
                    selected: &selected[..selected.len() - 1],
                },
                candidate,
                *layer,
                Some(*layer),
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
                    &RankContext {
                        strategy: request.strategy,
                        pair_map: &pair_map,
                        layer_map: &layer_map,
                        previous: &previous,
                        selected: &selected,
                    },
                    &excluded,
                    left,
                    right,
                    None,
                )
            });
        match best {
            Some(candidate) => {
                selected_ids.insert(candidate.master_id.clone());
                selected.push(candidate.master_id.clone());
                let layer = deepest_layer(candidate, &covered);
                covered.insert(layer);
                seats.push(make_seat(
                    &RankContext {
                        strategy: request.strategy,
                        pair_map: &pair_map,
                        layer_map: &layer_map,
                        previous: &previous,
                        selected: &selected[..selected.len() - 1],
                    },
                    candidate,
                    layer,
                    None,
                    false,
                ));
            }
            None => break,
        }
    }

    let layers: Vec<Layer> = covered.iter().copied().collect();

    // 缺口题：没有席位站上去的题、全池里能站上这一题的人不足两位而注定
    // 无法形成同题对立的题，以及上一轮在这一题上没能谈拢的题。
    let gaps: Vec<Layer> = LAYER_ORDER
        .iter()
        .copied()
        .filter(|layer| {
            !layers.contains(layer)
                || request.diverged.contains(layer)
                || pool
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.layers.contains(layer))
                    .count()
                    < 2
        })
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
    ctx: &RankContext<'_>,
    excluded: &BTreeSet<&str>,
    left: &Candidate,
    right: &Candidate,
    layer: Option<Layer>,
) -> std::cmp::Ordering {
    let penalty = |candidate: &Candidate| {
        if excluded.contains(candidate.master_id.as_str()) {
            EXCLUSION_PENALTY
        } else {
            0.0
        }
    };
    let left_penalty = penalty(left);
    let right_penalty = penalty(right);
    // 换批排除仍是最强优先级，避免为了「料多」把换人意图打穿。
    if left_penalty != right_penalty {
        return left_penalty
            .partial_cmp(&right_penalty)
            .unwrap_or(std::cmp::Ordering::Equal);
    }
    // 补某一题时，先比候选在该题上的积累深浅，再比策略分。
    if let Some(target) = layer {
        let left_depth = left.layer_depth.get(&target).copied().unwrap_or(0);
        let right_depth = right.layer_depth.get(&target).copied().unwrap_or(0);
        if left_depth != right_depth {
            return right_depth.cmp(&left_depth);
        }
    }
    let left_score = rank_score(ctx, left, layer);
    let right_score = rank_score(ctx, right, layer);
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
    ctx: &RankContext<'_>,
    candidate: &Candidate,
    layer: Layer,
    ranking_layer: Option<Layer>,
    pinned: bool,
) -> super::Seat {
    super::Seat {
        master_id: candidate.master_id.clone(),
        name: candidate.name.clone(),
        domain: candidate.domain.clone(),
        layers: candidate.layers.clone(),
        layer,
        score: rank_score(ctx, candidate, ranking_layer),
        pinned,
    }
}
