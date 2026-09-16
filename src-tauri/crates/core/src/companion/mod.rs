//! 主动助理：对新信号做一次轻量碰撞，产出关联、冲突与盲区洞察。
//!
//! 助理保持克制。主动助学默认关闭，关闭时不产生任何新洞察；开启后单个自然
//! 日内由助理推送的洞察数量不超过用户设定上限。碰撞只做一次模型调用，上下文
//! 取自思维网络中与信号相关的有限节点与大师框架，避免持续消耗。

pub mod collide;
pub mod growth;
pub mod repo;

use serde::{Deserialize, Serialize};

use crate::master::Layer;
use crate::network::GraphNode;

/// 默认每日推送上限。
pub const DEFAULT_DAILY_LIMIT: i64 = 5;
pub const MAX_DAILY_LIMIT: i64 = 50;
/// 默认纳入碰撞上下文的相关节点数上限。
pub const DEFAULT_CONTEXT_NODES: i64 = 6;
pub const MAX_CONTEXT_NODES: i64 = 20;
/// 碰撞上下文的字符预算，限制单次调用规模。
pub const MAX_CONTEXT_CHARS: usize = 1200;
/// 连续采纳达到该次数后提升为个人原则。
pub const PRINCIPLE_ADOPTION_THRESHOLD: i64 = 3;
/// 洞察来源：主动助理推送 / 固化识别。
pub const SOURCE_COMPANION: &str = "companion";
pub const SOURCE_CONSOLIDATION: &str = "consolidation";
pub const INSIGHT_STATUS_NEW: &str = "new";

/// 洞察类型：关联、冲突、盲区。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InsightKind {
    Relation,
    Conflict,
    Blindspot,
}

pub const INSIGHT_KINDS: [InsightKind; 3] = [
    InsightKind::Relation,
    InsightKind::Conflict,
    InsightKind::Blindspot,
];

impl InsightKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InsightKind::Relation => "relation",
            InsightKind::Conflict => "conflict",
            InsightKind::Blindspot => "blindspot",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            InsightKind::Relation => "关联",
            InsightKind::Conflict => "冲突",
            InsightKind::Blindspot => "盲区",
        }
    }

    pub fn parse(value: &str) -> Option<InsightKind> {
        let value = value.trim().to_ascii_lowercase();
        INSIGHT_KINDS
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value || kind.name() == value)
    }
}

/// 主动助学的触发规则。缺省字段回落到默认值，便于规则 JSON 向前兼容。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompanionRules {
    /// 新思考记录落库时是否触发碰撞。
    pub trigger_on_record: bool,
    /// 新采集内容落库时是否触发碰撞。
    pub trigger_on_capture: bool,
    /// 纳入上下文的相关节点最低激活度。
    pub min_activation: f64,
    /// 单次碰撞纳入的相关节点数上限。
    pub context_nodes: i64,
}

impl Default for CompanionRules {
    fn default() -> Self {
        Self {
            trigger_on_record: true,
            trigger_on_capture: true,
            min_activation: 0.0,
            context_nodes: DEFAULT_CONTEXT_NODES,
        }
    }
}

/// 主动助学设置。`used_today` 为当日已推送条数，便于界面展示剩余额度。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSettings {
    pub enabled: bool,
    pub daily_limit: i64,
    pub rules: CompanionRules,
    pub used_today: i64,
    pub updated_at: String,
}

/// 一条洞察。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub id: String,
    pub kind: InsightKind,
    pub title: String,
    pub summary: String,
    pub related_node_ids: Vec<String>,
    pub related_master_ids: Vec<String>,
    pub evidence: Vec<String>,
    pub status: String,
    pub action: String,
    pub reason: String,
    pub source: String,
    pub created_at: String,
}

/// 新建洞察的入参。
#[derive(Clone, Debug)]
pub struct NewInsight<'a> {
    pub kind: InsightKind,
    pub title: &'a str,
    pub summary: &'a str,
    pub related_node_ids: &'a [String],
    pub related_master_ids: &'a [String],
    pub evidence: &'a [String],
    pub source: &'a str,
}

/// 洞察读取过滤条件。
#[derive(Clone, Debug, Default)]
pub struct InsightFilter {
    pub kind: Option<InsightKind>,
    pub status: Option<String>,
    pub source: Option<String>,
}

/// 触发一次碰撞的信号。来源可以是思考记录或采集内容。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionSignal {
    pub source_kind: String,
    pub source_ref: String,
    pub content: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub layers: Vec<Layer>,
}

/// 一次碰撞的结果。`skipped` 说明本次为何没有产出。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionOutcome {
    pub generated: Vec<Insight>,
    pub pushed: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// 本次之后当日剩余额度。
    pub remaining: i64,
}

/// 主题聚合视图：同一 topic_key 下的判断演化。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicView {
    pub topic_key: String,
    pub record_count: i64,
    pub adopted_count: i64,
    pub latest_conclusion: String,
    pub latest_at: String,
    pub created_at: String,
}

/// 领域增长趋势。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainTrend {
    pub domain: String,
    pub count: i64,
}

/// 年轮概览：围绕炉温的四个成长指标。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RingOverview {
    pub top_nodes: Vec<GraphNode>,
    pub fastest_domains: Vec<DomainTrend>,
    pub new_edge_count: i64,
    pub principle_count: i64,
}

/// 一枚原则印章。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrincipleSeal {
    pub node_id: String,
    pub content: String,
    pub domains: Vec<String>,
    pub layers: Vec<Layer>,
    pub activation: f64,
    pub adopted_count: i64,
    pub created_at: String,
}
