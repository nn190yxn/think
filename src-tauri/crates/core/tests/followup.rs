//! 追问会话：锚点校验、文字截断、母会话不变、阵容继承与衍生连线。

use thought_forge_core::council::{
    followup, repo, FollowUpAnchor, Seat, Selection, Strategy,
};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::master::Layer;
use thought_forge_core::network::{recorder, repo as network_repo, GraphFilter, Relation};

fn memory_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn selection(master_ids: &[&str]) -> Selection {
    let seats: Vec<Seat> = master_ids
        .iter()
        .map(|id| Seat {
            master_id: (*id).to_string(),
            name: (*id).to_string(),
            domain: "测试领域".to_string(),
            layers: vec![Layer::Fa],
            layer: Layer::Fa,
            score: 1.0,
            pinned: false,
        })
        .collect();
    Selection {
        strategy: Strategy::Steady,
        size: seats.len(),
        seats,
        layers: vec![Layer::Fa],
        gaps: Vec::new(),
    }
}

fn parent_session(conn: &rusqlite::Connection) -> String {
    let id = repo::create_session(conn, "要不要换一条路走", &[], &[], Strategy::Steady)
        .expect("母会话可新建");
    repo::record_panel(conn, &id, 0, &selection(&["m-1", "m-2"]), &[])
        .expect("母会话阵容可记录");
    repo::finish_session(conn, &id, "先验证再决定", &["两种主张各执一词".to_string()])
        .expect("母会话可收敛");
    id
}

fn anchor(kind: &str, text: &str) -> FollowUpAnchor {
    FollowUpAnchor {
        kind: kind.to_string(),
        text: text.to_string(),
        master_id: None,
        round: None,
    }
}

#[test]
fn anchor_kind_is_restricted_and_required() {
    let conn = memory_db();
    let parent = parent_session(&conn);

    let unknown = followup::create_followup(&conn, &parent, &anchor("gossip", "随便一段"), "为什么", false);
    assert!(unknown.is_err(), "未知锚点类型应被拒绝");

    let empty_kind =
        followup::create_followup(&conn, &parent, &anchor("  ", "随便一段"), "为什么", false);
    assert!(empty_kind.is_err(), "空锚点类型应被拒绝");

    let empty_text = followup::create_followup(&conn, &parent, &anchor("answer", "   "), "为什么", false);
    assert!(empty_text.is_err(), "空锚点文字应被拒绝");

    let empty_question =
        followup::create_followup(&conn, &parent, &anchor("answer", "一段判断"), "  ", false);
    assert!(empty_question.is_err(), "空问句应被拒绝");
}

#[test]
fn overlong_anchor_text_is_truncated() {
    let conn = memory_db();
    let parent = parent_session(&conn);
    let long = "判".repeat(followup::ANCHOR_MAX_CHARS + 500);

    let session = followup::create_followup(&conn, &parent, &anchor("conclusion", &long), "为什么", false)
        .expect("超长锚点可照常创建");

    assert!(session.anchor_truncated, "应标注已截断");
    assert_eq!(
        session.anchor_text.chars().count(),
        followup::ANCHOR_MAX_CHARS
    );
    assert_eq!(session.anchor_kind, "conclusion");
    assert_eq!(session.parent_session_id.as_deref(), Some(parent.as_str()));
}

#[test]
fn creating_followup_leaves_parent_untouched() {
    let conn = memory_db();
    let parent = parent_session(&conn);
    let before = repo::get_session(&conn, &parent).unwrap();
    let before_panels = repo::panels(&conn, &parent).unwrap().len();
    let before_turns = repo::turns(&conn, &parent, None).unwrap().len();

    followup::create_followup(
        &conn,
        &parent,
        &anchor("divergence", "两种主张各执一词"),
        "哪一种更可行",
        true,
    )
    .expect("追问可创建");

    let after = repo::get_session(&conn, &parent).unwrap();
    assert_eq!(before.status, after.status);
    assert_eq!(before.conclusion, after.conclusion);
    assert_eq!(before.divergences, after.divergences);
    assert_eq!(before.updated_at, after.updated_at);
    assert_eq!(repo::panels(&conn, &parent).unwrap().len(), before_panels);
    assert_eq!(repo::turns(&conn, &parent, None).unwrap().len(), before_turns);
}

#[test]
fn inherited_panel_matches_parent_last_rotation() {
    let conn = memory_db();
    let parent = parent_session(&conn);
    let parent_panel = repo::latest_panel(&conn, &parent).unwrap().unwrap();

    let session = followup::create_followup(
        &conn,
        &parent,
        &anchor("answer", "先验证再决定"),
        "怎么验证",
        true,
    )
    .expect("追问可创建");

    assert!(session.panel_inherited);
    let panel = repo::latest_panel(&conn, &session.id).unwrap().unwrap();
    assert_eq!(panel.rotation, 0);
    assert_eq!(panel.master_ids, parent_panel.master_ids);
    assert_eq!(panel.pinned_ids, parent_panel.pinned_ids);
    assert_eq!(panel.strategy, parent_panel.strategy);
}

#[test]
fn inheritance_degrades_when_parent_has_no_panel() {
    let conn = memory_db();
    let parent = repo::create_session(&conn, "还没有阵容的问题", &[], &[], Strategy::Steady)
        .expect("母会话可新建");

    let session = followup::create_followup(
        &conn,
        &parent,
        &anchor("answer", "一段判断"),
        "追问一句",
        true,
    )
    .expect("追问可创建");

    assert!(!session.panel_inherited, "无阵容时应退化为常规选角");
    assert!(repo::latest_panel(&conn, &session.id).unwrap().is_none());
}

#[test]
fn followup_conclusion_derives_from_parent_conclusion() {
    let conn = memory_db();
    let parent = parent_session(&conn);
    let parent_outcome = recorder::record_session(&conn, &parent).expect("母会话可落网");

    let session = followup::create_followup(
        &conn,
        &parent,
        &anchor("conclusion", "先验证再决定"),
        "验证到什么程度才算够",
        true,
    )
    .expect("追问可创建");
    repo::finish_session(&conn, &session.id, "验证到能承受最坏结果", &[])
        .expect("追问可收敛");
    let followup_outcome = recorder::record_session(&conn, &session.id).expect("追问可落网");

    let graph = network_repo::get_graph(&conn, &GraphFilter::default()).expect("可读图谱");
    let linked = graph.edges.iter().any(|edge| {
        edge.relation == Relation::Derives
            && edge.from == parent_outcome.judgment_id
            && edge.to == followup_outcome.judgment_id
    });
    assert!(linked, "原结论节点到追问结论节点应有衍生连线");
}

#[test]
fn followup_prompt_restates_the_anchor() {
    let conn = memory_db();
    let parent = parent_session(&conn);
    let panel = repo::latest_panel(&conn, &parent).unwrap().unwrap();
    let request = followup::followup_prompt("怎么验证", &anchor("critique", "缺少成本估算"), &panel);

    assert_eq!(request.purpose, followup::FOLLOWUP_PURPOSE);
    assert!(request.system.contains("复述"), "系统提示词应要求复述锚点");
    assert!(request.user.contains("缺少成本估算"));
    assert!(request.user.contains("m-1"));
}
