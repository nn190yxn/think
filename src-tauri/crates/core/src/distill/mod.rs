//! 蒸馏流水线：六阶段状态机、五路提取、三重验证、技能单元生成与双通道入库。
//!
//! 流水线只依赖 [`crate::llm::ModelClient`] 抽象，因此离线环境可以用脚本化
//! 客户端完整验证从材料到可安装大师包的全链路。

pub mod intake;
pub mod pipeline;
pub mod repo;

use serde::{Deserialize, Serialize};

use crate::master::InstallOutcome;

/// 写入调用审计的各阶段用途标记。
pub const PURPOSE_SKELETON: &str = "distill_skeleton";
pub const PURPOSE_EXTRACT: &str = "distill_extract";
pub const PURPOSE_VERIFY: &str = "distill_verify";
pub const PURPOSE_COMPOSE: &str = "distill_compose";
pub const PURPOSE_STRESS: &str = "distill_stress";

/// 入库来源。
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_DISCOVERY: &str = "discovery";

pub const MAX_MATERIALS: usize = 40;
/// 单条材料交给模型的最大字符数，避免一本书撑爆上下文。
pub const MAX_MATERIAL_CHARS: usize = 4000;
/// 重复判定的相似度阈值。
pub const DUPLICATE_SIMILARITY_THRESHOLD: f64 = 0.6;
/// 压力测试通过率低于该值时在交付中标注风险。
pub const STRESS_PASS_THRESHOLD: f64 = 0.6;

/// 蒸馏阶段。顺序即执行顺序，`stage` 指针总是指向「下一个待执行阶段」。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DistillStage {
    Skeleton,
    Extract,
    Verify,
    Compose,
    Map,
    Stress,
    Deliver,
    Done,
}

pub const STAGE_ORDER: [DistillStage; 8] = [
    DistillStage::Skeleton,
    DistillStage::Extract,
    DistillStage::Verify,
    DistillStage::Compose,
    DistillStage::Map,
    DistillStage::Stress,
    DistillStage::Deliver,
    DistillStage::Done,
];

impl DistillStage {
    pub fn as_str(self) -> &'static str {
        match self {
            DistillStage::Skeleton => "skeleton",
            DistillStage::Extract => "extract",
            DistillStage::Verify => "verify",
            DistillStage::Compose => "compose",
            DistillStage::Map => "map",
            DistillStage::Stress => "stress",
            DistillStage::Deliver => "deliver",
            DistillStage::Done => "done",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DistillStage::Skeleton => "整体理解",
            DistillStage::Extract => "五路提取",
            DistillStage::Verify => "三重验证",
            DistillStage::Compose => "技能单元",
            DistillStage::Map => "技能地图",
            DistillStage::Stress => "压力测试",
            DistillStage::Deliver => "交付安装",
            DistillStage::Done => "完成",
        }
    }

    pub fn parse(value: &str) -> Option<DistillStage> {
        STAGE_ORDER
            .iter()
            .copied()
            .find(|stage| stage.as_str() == value.trim().to_ascii_lowercase())
    }

    pub fn index(self) -> usize {
        STAGE_ORDER.iter().position(|stage| *stage == self).unwrap_or(0)
    }

    /// 骨架确认门：阶段0 结束后需要用户确认才继续。
    pub fn is_gate(self) -> bool {
        matches!(self, DistillStage::Extract)
    }
}

/// 蒸馏任务状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistillState {
    Pending,
    AwaitingConfirmation,
    Running,
    Failed,
    Done,
}

impl DistillState {
    pub fn as_str(self) -> &'static str {
        match self {
            DistillState::Pending => "pending",
            DistillState::AwaitingConfirmation => "awaiting_confirmation",
            DistillState::Running => "running",
            DistillState::Failed => "failed",
            DistillState::Done => "done",
        }
    }

    pub fn parse(value: &str) -> Option<DistillState> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pending" => Some(DistillState::Pending),
            "awaiting_confirmation" => Some(DistillState::AwaitingConfirmation),
            "running" => Some(DistillState::Running),
            "failed" => Some(DistillState::Failed),
            "done" => Some(DistillState::Done),
            _ => None,
        }
    }
}

/// 五路提取器。各自独立成一次模型调用，保证视角互不污染。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractTrack {
    Framework,
    Principle,
    Case,
    Counterexample,
    Term,
}

pub const EXTRACT_TRACKS: [ExtractTrack; 5] = [
    ExtractTrack::Framework,
    ExtractTrack::Principle,
    ExtractTrack::Case,
    ExtractTrack::Counterexample,
    ExtractTrack::Term,
];

impl ExtractTrack {
    pub fn as_str(self) -> &'static str {
        match self {
            ExtractTrack::Framework => "framework",
            ExtractTrack::Principle => "principle",
            ExtractTrack::Case => "case",
            ExtractTrack::Counterexample => "counterexample",
            ExtractTrack::Term => "term",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ExtractTrack::Framework => "判断框架",
            ExtractTrack::Principle => "原则清单",
            ExtractTrack::Case => "案例",
            ExtractTrack::Counterexample => "反例",
            ExtractTrack::Term => "术语",
        }
    }

    pub fn instruction(self) -> &'static str {
        match self {
            ExtractTrack::Framework => {
                "提取他反复使用的判断框架：先看什么、按什么顺序看、如何下结论。"
            }
            ExtractTrack::Principle => {
                "提取他明确或反复体现的原则：什么该做、什么不该做、取舍标准是什么。"
            }
            ExtractTrack::Case => "提取他讲过的具体案例：背景、动作与结果，以及案例说明的道理。",
            ExtractTrack::Counterexample => {
                "提取他反对或警示的做法：什么样的做法会失败，失败机制是什么。"
            }
            ExtractTrack::Term => "提取他使用的专有术语与关键概念，并给出他的用法定义。",
        }
    }

    pub fn parse(value: &str) -> Option<ExtractTrack> {
        EXTRACT_TRACKS
            .iter()
            .copied()
            .find(|track| track.as_str() == value.trim().to_ascii_lowercase())
    }
}

/// 一次入库材料。文件与链接只登记路径或地址，文本型材料随任务保存正文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeMaterial {
    pub title: String,
    #[serde(default = "default_material_kind")]
    pub kind: String,
    #[serde(default)]
    pub source_ref: String,
    #[serde(default)]
    pub text: String,
}

fn default_material_kind() -> String {
    "note".to_string()
}

/// 总体理解阶段产出的骨架。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SkeletonDraft {
    pub summary: String,
    pub domain: String,
    pub layers: Vec<String>,
    pub themes: Vec<String>,
    pub angles: Vec<String>,
}

/// 提取阶段的候选。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CandidateDraft {
    pub id: String,
    pub track: String,
    pub title: String,
    pub summary: String,
    pub layer: String,
    pub evidence: Vec<String>,
}

/// 未通过三重验证的候选，保留可追溯的排除原因。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExcludedCandidate {
    pub id: String,
    pub track: String,
    pub title: String,
    pub stage: String,
    pub reason: String,
}

/// 技能单元草稿，四要素齐备才允许进入大师包。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SkillUnitDraft {
    pub candidate_id: String,
    pub title: String,
    pub layer: String,
    pub trigger_condition: String,
    pub steps: Vec<String>,
    pub mechanism: String,
    pub boundary: String,
    pub evidence: Vec<String>,
}

/// 技能地图分组。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SkillGroup {
    pub layer: String,
    pub name: String,
    pub titles: Vec<String>,
}

/// 技能单元之间的交叉链接。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SkillLink {
    pub from: String,
    pub to: String,
    pub relation: String,
}

/// 压力测试用例，含诱饵题。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StressCaseDraft {
    pub question: String,
    pub decoy: bool,
    pub expected: String,
    pub answer: String,
    pub passed: bool,
}

/// 检查点中的全部草稿。序列化后存入 `distill_jobs.checkpoint_json`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DistillDraft {
    pub skeleton: Option<SkeletonDraft>,
    pub extracted: Vec<CandidateDraft>,
    pub verified: Vec<CandidateDraft>,
    pub excluded: Vec<ExcludedCandidate>,
    pub units: Vec<SkillUnitDraft>,
    pub skill_map: Vec<SkillGroup>,
    pub links: Vec<SkillLink>,
    pub stress: Vec<StressCaseDraft>,
    pub stress_pass_rate: f64,
    pub pack_dir: Option<String>,
    pub outcome: Option<InstallOutcome>,
}

/// 蒸馏任务摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistillJobView {
    pub id: String,
    pub source_kind: String,
    pub source_ref: String,
    pub master_id: String,
    pub master_name: String,
    pub domain: String,
    pub output_dir: String,
    pub stage: DistillStage,
    pub stage_name: String,
    pub state: DistillState,
    pub material_count: i64,
    pub model_calls: i64,
    pub error_code: Option<String>,
    pub updated_at: String,
    pub created_at: String,
}

/// 蒸馏任务详情，附带检查点草稿。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistillDetail {
    pub job: DistillJobView,
    pub draft: DistillDraft,
}

/// 入库任务状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntakeState {
    Queued,
    AwaitingConfirmation,
    Confirmed,
    Rejected,
    Distilling,
    Done,
    Failed,
}

impl IntakeState {
    pub fn as_str(self) -> &'static str {
        match self {
            IntakeState::Queued => "queued",
            IntakeState::AwaitingConfirmation => "awaiting_confirmation",
            IntakeState::Confirmed => "confirmed",
            IntakeState::Rejected => "rejected",
            IntakeState::Distilling => "distilling",
            IntakeState::Done => "done",
            IntakeState::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<IntakeState> {
        match value.trim().to_ascii_lowercase().as_str() {
            "queued" => Some(IntakeState::Queued),
            "awaiting_confirmation" => Some(IntakeState::AwaitingConfirmation),
            "confirmed" => Some(IntakeState::Confirmed),
            "rejected" => Some(IntakeState::Rejected),
            "distilling" => Some(IntakeState::Distilling),
            "done" => Some(IntakeState::Done),
            "failed" => Some(IntakeState::Failed),
            _ => None,
        }
    }
}

/// 动态信号状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalStatus {
    Pending,
    Accepted,
    Rejected,
}

impl SignalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SignalStatus::Pending => "pending",
            SignalStatus::Accepted => "accepted",
            SignalStatus::Rejected => "rejected",
        }
    }
}

/// 资料重叠度评估结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlapSummary {
    pub materials: i64,
    pub duplicates: i64,
    pub max_ratio: f64,
    pub notes: Vec<String>,
}

/// 入库任务视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeJobView {
    pub id: String,
    pub master_ref: String,
    pub master_name: String,
    pub domain: String,
    pub mode: String,
    pub state: IntakeState,
    pub material_count: i64,
    pub accepted_count: i64,
    pub rejected_count: i64,
    pub overlap: OverlapSummary,
    pub updated_at: String,
    pub created_at: String,
}

/// 待确认清单中的一条材料。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalView {
    pub id: String,
    pub job_id: String,
    pub master_id: String,
    pub title: String,
    pub source_ref: String,
    pub kind: String,
    pub text: String,
    pub status: SignalStatus,
    pub decision_reason: String,
    pub overlap_ratio: f64,
    pub discovered_at: String,
}

/// 主动搜集设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySettings {
    pub enabled: bool,
    pub schedule: serde_json::Value,
    pub updated_at: String,
}

/// 单次主动搜集的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryOutcome {
    pub triggered: bool,
    pub reason: Option<String>,
    pub discovered: i64,
    pub saved: i64,
    pub pending: i64,
    pub job_id: Option<String>,
}
