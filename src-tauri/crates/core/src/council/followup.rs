//! 追问会话：围绕母会话里的一段判断开一场带锚点的新会诊。
//!
//! 追问以新会话承载，母会话的任何字段与轮次记录都不被改写，历史可完整回溯。

use rusqlite::Connection;

use crate::error::{CoreError, CoreResult};
use crate::llm::ModelRequest;

use super::{repo, FollowUpAnchor, PanelView, SessionView};

/// 锚点文字上限，超出按上限截断。
pub const ANCHOR_MAX_CHARS: usize = 2000;

/// 允许的锚点类型：结论、作答、质询、分歧。
pub const ANCHOR_KINDS: [&str; 4] = ["conclusion", "answer", "critique", "divergence"];

/// 追问提示词与调用的用途标识。
pub const FOLLOWUP_PURPOSE: &str = "council_followup";

fn anchor_label(kind: &str) -> &'static str {
    match kind {
        "conclusion" => "结论",
        "answer" => "作答",
        "critique" => "质询",
        "divergence" => "分歧",
        _ => "判断",
    }
}

/// 校验锚点类型，返回规范化后的取值。
fn normalize_kind(kind: &str) -> CoreResult<&str> {
    let trimmed = kind.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("追问锚点类型不能为空".into()));
    }
    ANCHOR_KINDS
        .iter()
        .copied()
        .find(|allowed| allowed.eq_ignore_ascii_case(trimmed))
        .ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "未知锚点类型：{trimmed}，只支持 conclusion/answer/critique/divergence"
            ))
        })
}

/// 新建追问会话。锚点类型与文字必填，文字超长按上限截断。
///
/// `inherit_panel` 为真时复制母会话最后一个阵容；母会话尚无阵容时退化为常规
/// 选角，并把 `panel_inherited` 置假，界面据此说明本次未继承阵容。
pub fn create_followup(
    conn: &Connection,
    parent_session_id: &str,
    anchor: &FollowUpAnchor,
    question: &str,
    inherit_panel: bool,
) -> CoreResult<SessionView> {
    let kind = normalize_kind(&anchor.kind)?;
    let text = anchor.text.trim();
    if text.is_empty() {
        return Err(CoreError::InvalidInput("追问锚点文字不能为空".into()));
    }
    let question = question.trim();
    if question.is_empty() {
        return Err(CoreError::InvalidInput("追问问句不能为空".into()));
    }

    let parent = repo::get_session(conn, parent_session_id)?;
    let anchor_text: String = text.chars().take(ANCHOR_MAX_CHARS).collect();
    let anchor_truncated = anchor_text.chars().count() < text.chars().count();

    let parent_rotation = repo::latest_rotation(conn, parent_session_id)?;
    let inherited = inherit_panel && parent_rotation.is_some();

    let session_id = repo::create_followup_session(
        conn,
        &repo::NewFollowUp {
            parent_session_id: &parent.id,
            anchor_kind: kind,
            anchor_text: &anchor_text,
            anchor_master_id: anchor.master_id.as_deref(),
            anchor_round: anchor.round,
            anchor_truncated,
            panel_inherited: inherited,
            question,
            domains: &parent.domains,
            layers: &parent.layers,
            strategy: parent.strategy,
        },
    )?;

    if let Some(source_rotation) = parent_rotation.filter(|_| inherited) {
        repo::copy_panel(conn, parent_session_id, source_rotation, &session_id, 0)?;
    }

    repo::get_session(conn, &session_id)
}

/// 追问提示词：先复述锚点要义再回答，避免答非所问。
pub fn followup_prompt(
    question: &str,
    anchor: &FollowUpAnchor,
    panel: &PanelView,
) -> ModelRequest {
    let label = anchor_label(&anchor.kind);
    let participants = if panel.master_ids.is_empty() {
        "（本次为常规选角）".to_string()
    } else {
        panel.master_ids.join("、")
    };
    ModelRequest::new(
        FOLLOWUP_PURPOSE,
        "这是一次针对既有判断的追问，不是全新的会诊。请先用一句话复述被追问的\
         锚点要义，确认你理解了追问对象，再围绕追问作答；若追问触及你此前未言明\
         的前提，请显式说明。",
        format!(
            "被追问的{label}：\n{text}\n\n追问：{question}\n\n本席阵容：{participants}",
            text = anchor.text.trim(),
        ),
    )
    .with_prompt_version(super::orchestrator::PROMPT_VERSION)
}
