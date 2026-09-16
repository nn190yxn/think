//! 自我蒸馏：把用户自己的历史判断蒸成「你」这位大师的初稿。
//!
//! 与蒸馏流水线不同，自我蒸馏的材料来自本机的思考记录，产出逐条确认后才安装。
//! 安装后的「你」作为保留席位参与之后每一次会诊，用来对照「现在的你」与
//! 「当时的你」。

pub mod repo;
pub mod service;

use serde::{Deserialize, Serialize};

use crate::master::Layer;

/// 用户本人大师的固定标识与名称。标识必须满足大师包 id 规则。
pub const SELF_MASTER_ID: &str = "self";
pub const SELF_MASTER_NAME: &str = "你";
pub const SELF_DOMAIN: &str = "自我";

/// 解锁自我蒸馏所需的思考记录条数。
pub const UNLOCK_RECORD_COUNT: i64 = 20;
/// 单次自我蒸馏最多读取的记录条数。
pub const MAX_RECORDS: i64 = 40;
/// 单次自我蒸馏最多产出的候选条目。
pub const MAX_ITEMS: usize = 24;
/// 写入模型调用审计的用途标记。
pub const PURPOSE_SELF: &str = "self_distill";
/// 席位开关的设置键。
pub const SEAT_SETTING_KEY: &str = "self.seat_enabled";

/// 候选条目状态：待确认、已采纳、已剔除。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelfItemState {
    Pending,
    Accepted,
    Rejected,
}

impl SelfItemState {
    pub fn as_str(self) -> &'static str {
        match self {
            SelfItemState::Pending => "pending",
            SelfItemState::Accepted => "accepted",
            SelfItemState::Rejected => "rejected",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SelfItemState::Pending => "待确认",
            SelfItemState::Accepted => "已采纳",
            SelfItemState::Rejected => "已剔除",
        }
    }
}

/// 草稿状态：可确认、已安装、失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfDraftState {
    Ready,
    Installed,
    Failed,
}

impl SelfDraftState {
    pub fn as_str(self) -> &'static str {
        match self {
            SelfDraftState::Ready => "ready",
            SelfDraftState::Installed => "installed",
            SelfDraftState::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<SelfDraftState> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ready" => Some(SelfDraftState::Ready),
            "installed" => Some(SelfDraftState::Installed),
            "failed" => Some(SelfDraftState::Failed),
            _ => None,
        }
    }
}

/// 一条历史思考记录的只读快照，作为自我蒸馏的材料。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfRecord {
    pub id: String,
    pub question: String,
    pub conclusion: String,
    pub adopted: bool,
    pub reason: String,
    pub domains: Vec<String>,
    pub layers: Vec<Layer>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfItemView {
    pub id: String,
    pub draft_id: String,
    pub ordinal: i64,
    pub title: String,
    pub layer: Layer,
    pub trigger_condition: String,
    pub steps: Vec<String>,
    pub mechanism: String,
    pub boundary: String,
    /// 支撑该条的历史记录摘要，供界面展示来源。
    pub evidence: Vec<String>,
    pub source_record_id: Option<String>,
    pub status: String,
    pub status_label: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfDraftView {
    pub id: String,
    pub status: String,
    pub record_count: i64,
    pub master_id: String,
    pub model_calls: i64,
    pub error_code: Option<String>,
    pub note: String,
    pub updated_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfDraftDetail {
    pub draft: SelfDraftView,
    pub items: Vec<SelfItemView>,
    pub pending_count: i64,
    pub accepted_count: i64,
    pub rejected_count: i64,
}

/// 铜镜入口所需的就绪度信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfReadiness {
    pub record_count: i64,
    pub required: i64,
    pub unlocked: bool,
    pub installed: bool,
    pub seat_enabled: bool,
    pub current_version: i64,
    pub latest_draft: Option<SelfDraftView>,
}
