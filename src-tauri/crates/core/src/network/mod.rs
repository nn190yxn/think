//! 思维网络：认知节点、认知关系、激活传播、衰减与固化。
//!
//! 这一层是全系统的认知汇聚层，会诊、蒸馏、采集的产出最终都落成节点与连线。
//! 激活度采用带时间衰减的累计值，固化把短期共激活沉淀为长期连线权重。

pub mod cluster;
pub mod consolidate;
pub mod recorder;
pub mod repo;

use serde::{Deserialize, Serialize};

use crate::master::Layer;

/// 默认半衰期：七天。可在设置中按小时调整。
pub const DEFAULT_HALF_LIFE_HOURS: f64 = 168.0;
pub const HALF_LIFE_SETTING_KEY: &str = "activation_half_life_hours";

/// 一跳邻居获得的激活增量系数。
pub const NEIGHBOR_FACTOR: f64 = 0.5;

/// 固化时每条共激活连线的强化步长。
pub const STRENGTHEN_STEP: f64 = 0.08;
/// 低于该权重且长期未激活的连线进入衰减候选。
pub const MIN_EDGE_WEIGHT: f64 = 0.05;
/// 连线视为「长期未激活」的天数。
pub const EDGE_STALE_DAYS: i64 = 30;
/// 节点合并的内容相似度阈值。
pub const MERGE_SIMILARITY: f64 = 0.85;
/// 单次固化的处理上限，避免长时间占用。
pub const MAX_CONSOLIDATION_BATCH: i64 = 2000;

/// 节点类型：念头、判断、框架、原则、问题、证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Idea,
    Judgment,
    Framework,
    Principle,
    Question,
    Evidence,
}

pub const NODE_KINDS: [NodeKind; 6] = [
    NodeKind::Idea,
    NodeKind::Judgment,
    NodeKind::Framework,
    NodeKind::Principle,
    NodeKind::Question,
    NodeKind::Evidence,
];

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Idea => "idea",
            NodeKind::Judgment => "judgment",
            NodeKind::Framework => "framework",
            NodeKind::Principle => "principle",
            NodeKind::Question => "question",
            NodeKind::Evidence => "evidence",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            NodeKind::Idea => "念头",
            NodeKind::Judgment => "判断",
            NodeKind::Framework => "框架",
            NodeKind::Principle => "原则",
            NodeKind::Question => "问题",
            NodeKind::Evidence => "证据",
        }
    }

    pub fn parse(value: &str) -> Option<NodeKind> {
        NODE_KINDS
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value.trim().to_ascii_lowercase())
    }
}

/// 关系类型：支持、冲突、衍生、类比、应用。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    Supports,
    Conflicts,
    Derives,
    Analogous,
    Applies,
}

pub const RELATIONS: [Relation; 5] = [
    Relation::Supports,
    Relation::Conflicts,
    Relation::Derives,
    Relation::Analogous,
    Relation::Applies,
];

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Supports => "supports",
            Relation::Conflicts => "conflicts",
            Relation::Derives => "derives",
            Relation::Analogous => "analogous",
            Relation::Applies => "applies",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Relation::Supports => "支持",
            Relation::Conflicts => "冲突",
            Relation::Derives => "衍生",
            Relation::Analogous => "类比",
            Relation::Applies => "应用",
        }
    }

    /// 冲突关系天然双向可见，其余关系保持方向语义。
    pub fn is_symmetric(self) -> bool {
        matches!(self, Relation::Conflicts)
    }

    pub fn parse(value: &str) -> Option<Relation> {
        RELATIONS
            .iter()
            .copied()
            .find(|relation| relation.as_str() == value.trim().to_ascii_lowercase())
    }
}

/// 一个节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtNode {
    pub id: String,
    pub kind: NodeKind,
    pub content: String,
    pub normalized_content: String,
    pub source_kind: String,
    pub source_ref: String,
    pub domains: Vec<String>,
    pub layers: Vec<Layer>,
    pub activation: f64,
    pub activation_updated_at: String,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    /// 最近一次固化把该节点归入的社区；尚未聚类时为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    pub created_at: String,
}

/// 一条连线（不含对端信息）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtEdge {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub relation: Relation,
    pub weight: f64,
    pub co_activation_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activated_at: Option<String>,
    pub status: String,
    pub created_at: String,
}

/// 节点详情里的一条连线，带方向与对端摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeLink {
    pub edge_id: String,
    pub relation: Relation,
    pub weight: f64,
    pub status: String,
    /// out 表示从当前节点指出，in 表示指向当前节点，both 表示对称关系。
    pub direction: String,
    pub peer_id: String,
    pub peer_kind: NodeKind,
    pub peer_content: String,
}

/// 一次激活记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationView {
    pub id: String,
    pub session_id: Option<String>,
    pub increment: f64,
    pub occurred_at: String,
}

/// 节点详情：节点本体、直接连线与激活来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    pub node: ThoughtNode,
    pub links: Vec<NodeLink>,
    pub activations: Vec<ActivationView>,
}

/// 图谱中的节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    pub content: String,
    pub domains: Vec<String>,
    pub layers: Vec<Layer>,
    pub activation: f64,
    pub activation_updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
}

/// 图谱中的连线。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub relation: Relation,
    pub weight: f64,
}

/// 图谱快照。节点数超过上限时截断，界面据此提示。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// 当前可见节点所属的社区。
    pub clusters: Vec<ClusterView>,
    pub total_nodes: i64,
    pub truncated: bool,
}

/// 一个认知社区及其可见成员。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterView {
    pub id: String,
    pub label: String,
    pub domain: String,
    pub layer: String,
    pub member_count: i64,
    pub member_ids: Vec<String>,
}

/// 图谱过滤条件。
#[derive(Debug, Clone, Default)]
pub struct GraphFilter {
    pub domain: Option<String>,
    pub layer: Option<Layer>,
    pub kind: Option<NodeKind>,
    /// 只返回属于该社区的节点。
    pub cluster_id: Option<String>,
    /// 只返回激活度不低于该值的节点。
    pub min_activation: Option<f64>,
    /// 节点数上限，内部会夹到安全区间。
    pub limit: Option<i64>,
}

/// 写入节点的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeUpsert {
    pub node_id: String,
    /// created 表示新建，matched 表示命中同内容节点并复用。
    pub outcome: String,
}

/// 写入连线的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeUpsert {
    pub edge_id: String,
    pub created: bool,
    pub weight: f64,
}

/// 一次唤醒的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationOutcome {
    /// 被直接唤醒的节点数。
    pub activated: usize,
    /// 因一跳传播被唤醒的邻居数。
    pub propagated: usize,
    /// 因二跳及以后传播被唤醒的节点数；跳数为一时为 0。
    pub propagated_far: usize,
    /// 实际传播到的跳数，至少为 1。
    pub hops: usize,
}

/// 会诊写入网络的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordOutcome {
    pub record_id: String,
    pub judgment_id: String,
    pub framework_ids: Vec<String>,
    pub divergence_ids: Vec<String>,
    pub linked_prior: Vec<String>,
    pub activated: usize,
}

/// 一次固化的处理明细。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationReport {
    pub run_id: String,
    pub mode: String,
    pub strengthened_count: i64,
    pub decayed_count: i64,
    pub merged_count: i64,
    pub conflict_count: i64,
    /// 本次划分出的社区数；聚类关闭或无社区时为 0。
    pub cluster_count: i64,
    /// 被合并掉的节点 id 及其替代者。
    pub merged: Vec<(String, String)>,
    /// 本次识别出的冲突：同一对节点同时有支持与冲突连线。
    pub conflicts: Vec<(String, String)>,
    pub started_at: String,
    pub finished_at: String,
}

/// 固化历史。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationRunView {
    pub id: String,
    pub mode: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub strengthened_count: i64,
    pub decayed_count: i64,
    pub merged_count: i64,
    pub conflict_count: i64,
}

/// 一条思考记录。同一 topic_key 的多条记录构成判断演化链。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtRecordView {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub question: String,
    pub topic_key: String,
    pub domains: Vec<String>,
    pub layers: Vec<Layer>,
    pub conclusion: String,
    pub adopted: bool,
    pub reason: String,
    pub created_at: String,
}
