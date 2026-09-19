//! 骑士团会诊：候选池评分、轮换策略选角、会诊会话与编排。

pub mod orchestrator;
pub mod pairings;
pub mod pool;
pub mod repo;
pub mod scoring;
pub mod select;
pub mod conclusion;
pub mod control;
pub mod divergence;
pub mod echo;
pub mod speech;
pub mod tuning;
pub mod followup;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::master::{Layer, LAYER_ORDER};

/// 未指定人数时按六层各取一位。
pub const DEFAULT_PANEL_SIZE: usize = 6;
pub const MIN_PANEL_SIZE: usize = 4;
pub const MAX_PANEL_SIZE: usize = 8;
/// 层次覆盖的硬下限，任何阵容都必须满足。
pub const MIN_LAYER_COVERAGE: usize = 3;

/// 轮换策略：稳妥看相关度，碰撞看对立度，意外看领域距离。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    Steady,
    Clash,
    Serendipity,
}

impl Strategy {
    pub fn parse(value: &str) -> Option<Strategy> {
        match value.trim().to_ascii_lowercase().as_str() {
            "steady" | "稳妥" => Some(Strategy::Steady),
            "clash" | "碰撞" => Some(Strategy::Clash),
            "serendipity" | "意外" => Some(Strategy::Serendipity),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Strategy::Steady => "steady",
            Strategy::Clash => "clash",
            Strategy::Serendipity => "serendipity",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Strategy::Steady => "稳妥",
            Strategy::Clash => "碰撞",
            Strategy::Serendipity => "意外",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Strategy::Steady => "优先选取与当前话题相关度最高的大师",
            Strategy::Clash => "优先选取彼此观点最对立的大师",
            Strategy::Serendipity => "优先选取与当前话题领域距离最远的大师",
        }
    }
}

/// 候选池中的一位大师及其三项评分。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub master_id: String,
    pub name: String,
    pub domain: String,
    pub layers: Vec<Layer>,
    /// 每题（层次）的积累深度，即该题的技能单元数；没有单元的题不出现。
    pub layer_depth: BTreeMap<Layer, usize>,
    /// 与话题标签、大师文本的匹配度。
    pub relevance: f64,
    /// 与其他候选的最大对立度，供碰撞策略排序。
    pub opposition: f64,
    /// 与话题领域的图谱距离，供意外策略排序。
    pub domain_distance: f64,
}

/// 按层级组织的大师候选池。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidatePool {
    pub question: String,
    pub domains: Vec<String>,
    /// 话题分词结果，便于界面解释评分来源。
    pub topic_tokens: Vec<String>,
    pub candidates: Vec<Candidate>,
    /// 完全没有大师的层次。
    pub missing_layers: Vec<Layer>,
}

/// 一个席位。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seat {
    pub master_id: String,
    pub name: String,
    pub domain: String,
    pub layers: Vec<Layer>,
    /// 该席位主要代表的层次。
    pub layer: Layer,
    /// 该席位在本次策略下的排序分。
    pub score: f64,
    /// 是否为用户保留席位。
    pub pinned: bool,
}

/// 一次选角结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub strategy: Strategy,
    pub size: usize,
    pub seats: Vec<Seat>,
    /// 入席大师覆盖到的层次。
    pub layers: Vec<Layer>,
    /// 因候选不足或人数受限而空缺的层次。
    pub gaps: Vec<Layer>,
}

impl Selection {
    pub fn master_ids(&self) -> Vec<String> {
        self.seats.iter().map(|seat| seat.master_id.clone()).collect()
    }
}

/// 席位被指派到的题：一位大师在本轮阵容中负责回答哪一问。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatRef {
    pub master_id: String,
    pub layer: Layer,
}

/// 席位主要代表的层次：取大师声明层次里最靠抽象端的一层。
/// 只用于历史阵容缺少席位指派时的回退，正常路径以阵容记录的指派为准。
pub fn primary_layer(layers: &[Layer]) -> Layer {
    LAYER_ORDER
        .iter()
        .copied()
        .find(|layer| layers.contains(layer))
        .or_else(|| layers.first().copied())
        .unwrap_or(Layer::Fa)
}

/// 一条未谈拢的地方：分歧落在哪一题，以及分歧本身的人话描述。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DivergenceView {
    pub layer: Layer,
    pub text: String,
}

/// 一个席位在自己那一题上的立场摘要，取自该席位最后一轮成功发言的开头。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StanceView {
    pub master_id: String,
    pub master_name: String,
    pub layer: Layer,
    pub summary: String,
}

/// 同一题与上一次同主题会诊相比的立场变化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StanceChange {
    pub layer: Layer,
    pub master_id: String,
    pub master_name: String,
    /// 上一轮在这一题上发言的人；本轮新谈或停谈时为空。
    pub previous_master_name: Option<String>,
    /// 本轮立场摘要；停谈时为空。
    pub summary: String,
    /// 上一轮立场摘要；新谈时为空。
    pub previous_summary: Option<String>,
    /// 两轮摘要的用词重合度，0 到 1；只在一方缺席时为 0。
    pub similarity: f64,
    /// same 延续、adjusted 调整、shifted 转向、new 新谈、dropped 停谈。
    pub change: String,
}

/// 会诊阵容的一次轮次。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelView {
    pub rotation: i64,
    pub strategy: Strategy,
    pub master_ids: Vec<String>,
    pub pinned_ids: Vec<String>,
    /// 与 master_ids 同序的席位指派；历史阵容为空，读取方按大师层次回退。
    pub seats: Vec<SeatRef>,
    pub layers: Vec<Layer>,
    pub gaps: Vec<Layer>,
    pub created_at: String,
}

impl PanelView {
    /// 本轮阵容给这位大师指派的题。
    pub fn layer_of(&self, master_id: &str) -> Option<Layer> {
        self.seats
            .iter()
            .find(|seat| seat.master_id == master_id)
            .map(|seat| seat.layer)
    }
}

/// 一轮发言。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnView {
    pub id: String,
    pub round: i64,
    pub panel_rotation: i64,
    /// answer 第一轮独立作答，cross 第二轮交叉质询，synthesis 收敛裁决。
    pub role: String,
    pub master_id: Option<String>,
    pub master_version: Option<i64>,
    pub content: String,
    pub citations: Vec<String>,
    /// 生成该轮发言时使用的提示词模板版本。
    pub prompt_version: String,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: String,
    pub question: String,
    pub domains: Vec<String>,
    pub layers: Vec<Layer>,
    pub strategy: Strategy,
    pub status: String,
    pub conclusion: String,
    /// 未谈拢的地方，每条标明它属于哪一题。
    pub divergences: Vec<DivergenceView>,
    pub rotation_count: i64,
    pub turn_count: i64,
    /// 追问会话的母会话标识；普通会诊为空。
    pub parent_session_id: Option<String>,
    pub anchor_kind: String,
    pub anchor_text: String,
    pub anchor_master_id: Option<String>,
    pub anchor_round: Option<i64>,
    /// 锚点文字因超长被截断时为真。
    pub anchor_truncated: bool,
    /// 是否成功继承了母会话阵容。
    pub panel_inherited: bool,
    /// 配额超限时采用的降级策略，空串表示未触发压缩。
    pub quota_policy: String,
    /// 被压缩后的轮次上限。
    pub quota_max_rounds: Option<i64>,
    /// 被压缩后的席位上限。
    pub quota_max_seats: Option<i64>,
    /// 压缩原因的人话说明。
    pub quota_reason: String,
    /// 本次会诊是否包含「你」的席位。
    pub self_seat_included: bool,
    /// 是否已请求取消；取消在当前轮次结束时生效。
    pub cancel_requested: bool,
    /// 取消生效时刻。
    pub cancelled_at: Option<String>,
    /// 最近一次心跳时刻，用于判定会话是否中断。
    pub heartbeat_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionView,
    pub panels: Vec<PanelView>,
    pub turns: Vec<TurnView>,
    pub metrics: Vec<RoundMetric>,
}

/// 一轮讨论的分歧指标，按 (session_id, panel_rotation, round) 唯一。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundMetric {
    pub session_id: String,
    pub panel_rotation: i64,
    pub round: i64,
    pub participant_count: i64,
    pub avg_similarity: f64,
    pub min_similarity: f64,
    pub divergence: f64,
    pub converged: bool,
    /// 本轮分歧判定方式：lexical、polarity 或 hybrid。
    pub method: String,
    /// 极性判定不可用而回退到词面判定时为真。
    pub fell_back: bool,
    pub created_at: String,
}

/// 某位大师的入席历史。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterHistoryEntry {
    pub session_id: String,
    pub question: String,
    pub round: i64,
    pub panel_rotation: i64,
    pub role: String,
    pub master_version: Option<i64>,
    pub content: String,
    pub created_at: String,
}

/// 一次会诊执行的产出摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouncilOutcome {
    pub session_id: String,
    pub rotation: i64,
    pub answered: usize,
    pub failed: usize,
    pub conclusion: String,
    pub divergences: Vec<DivergenceView>,
    /// 实际执行的讨论轮次数，含作答轮，不含收敛裁决。
    pub rounds: usize,
    pub metrics: Vec<RoundMetric>,
}

/// 一个席位在某一轮的发言。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSpeech {
    pub round: i64,
    /// answer 为独立作答，cross 为交叉质询。
    pub role: String,
    pub content: String,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

/// 逐席发言视图：按席位归组该席位在各轮的发言。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatSpeech {
    pub master_id: String,
    pub master_name: String,
    pub layer: Layer,
    /// 全部成功为 answered，存在失败为 failed，尚无发言为 pending。
    pub status: String,
    pub rounds: Vec<RoundSpeech>,
}

/// 追问锚点：指向母会话里被追问的那段判断。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowUpAnchor {
    /// 取值限 conclusion、answer、critique、divergence 四类。
    pub kind: String,
    pub text: String,
    pub master_id: Option<String>,
    pub round: Option<i64>,
}

/// 检索快照视图。master_id 为空表示共享背景。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
    pub fetched_at: String,
    pub has_body: bool,
    /// 该条外部资料是否命中注入特征。
    pub flagged: bool,
    pub master_id: Option<String>,
    pub round: i64,
}

/// 会诊结论详情页的六段数据：结论、分歧、收敛过程、逐席依据、外部来源、演化链。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConclusionView {
    pub session: SessionView,
    pub metrics: Vec<RoundMetric>,
    pub speeches: Vec<SeatSpeech>,
    pub sources: Vec<SourceView>,
    /// 生成该结论时使用的提示词模板版本。
    pub prompt_version: String,
    /// 同一主题的其它会诊，按时间倒序。
    pub history: Vec<SessionView>,
    /// 每题立场与上一次同主题会诊相比的变化；没有可比记录时为空。
    pub stance_changes: Vec<StanceChange>,
    /// 本场会诊已记录的模型调用次数。
    pub llm_calls: i64,
    /// 本场会诊已记录的检索调用次数。
    pub search_calls: i64,
    /// 按本场调用次数与当前单价估算的费用，整数微元。
    pub cost_micros: i64,
    pub currency: String,
    /// 是否至少配置了一项非零单价；为假时费用按零计。
    pub priced: bool,
}
