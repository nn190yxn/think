//! 回音检测与原则撤销：达阈值留痕、撤销后不再进上下文、内容与原因仍可读。

use thought_forge_core::council::echo;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::master::Layer;
use thought_forge_core::network::repo as network_repo;
use thought_forge_core::network::NodeKind;
use thought_forge_core::network::repo::NewNode;

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn node(conn: &rusqlite::Connection, kind: NodeKind, content: &str, source: &str) -> String {
    network_repo::upsert_node(
        conn,
        &NewNode {
            kind,
            content,
            source_kind: "test",
            source_ref: source,
            domains: &["自我".to_string()],
            layers: &[Layer::Dao],
        },
    )
    .expect("节点可写入")
    .node_id
}

#[test]
fn echo_records_hint_when_overlap_reaches_threshold() {
    let conn = db();
    let principle = node(
        &conn,
        NodeKind::Principle,
        "先看长期价值，再决定要不要短期让步",
        "p1",
    );
    let judgment = node(
        &conn,
        NodeKind::Judgment,
        "先看长期价值，再决定要不要短期让步",
        "j1",
    );

    let hits = echo::detect_echo(&conn, &judgment).expect("可检测回音");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].node_id, principle);
    assert!(hits[0].overlap >= 0.8, "同文应达到阈值");

    let hints: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM insights WHERE kind = 'echo' AND action = ?1",
            [&judgment],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hints, 1, "达阈值的命中应留一条回音提示");

    // 重复检测不重复留痕。
    echo::detect_echo(&conn, &judgment).expect("可重复检测");
    let again: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM insights WHERE kind = 'echo'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(again, 1);
}

#[test]
fn echo_stays_silent_below_threshold() {
    let conn = db();
    node(
        &conn,
        NodeKind::Principle,
        "凡是需要长期投入的事都要先小步验证再放大",
        "p1",
    );
    let judgment = node(&conn, NodeKind::Judgment, "今天天气不错适合出门散步", "j1");

    let hits = echo::detect_echo(&conn, &judgment).expect("可检测回音");
    assert!(hits.iter().all(|hit| hit.overlap < 0.8));
    let hints: i64 = conn
        .query_row("SELECT COUNT(*) FROM insights", [], |row| row.get(0))
        .unwrap();
    assert_eq!(hints, 0, "未达阈值不应留痕");
}

#[test]
fn revoked_principle_leaves_context_but_keeps_content_and_reason() {
    let conn = db();
    let principle = node(
        &conn,
        NodeKind::Principle,
        "先看长期价值，再决定要不要短期让步",
        "p1",
    );
    assert_eq!(echo::active_principles(&conn, 10).unwrap().len(), 1);

    echo::revoke_principle(&conn, &principle, "这条判断已经被新证据推翻").expect("可撤销");
    assert!(
        echo::active_principles(&conn, 10).unwrap().is_empty(),
        "撤销后不再进入会诊上下文"
    );
    let revoked = echo::revoked_principles(&conn, 10).unwrap();
    assert_eq!(revoked.len(), 1);
    assert!(revoked[0].content.contains("先看长期价值"), "内容仍可读");
    assert!(revoked[0].content.contains("新证据"), "撤销原因仍可读");

    // 幂等：重复撤销不报错，也不改写首次原因。
    echo::revoke_principle(&conn, &principle, "另一个原因").expect("重复撤销幂等");
    let reason = echo::revoked_principles(&conn, 10).unwrap()[0].content.clone();
    assert!(reason.contains("新证据"));
}

#[test]
fn revoke_rejects_non_principle_or_missing_node() {
    let conn = db();
    let judgment = node(&conn, NodeKind::Judgment, "一个普通判断", "j1");
    let error = echo::revoke_principle(&conn, &judgment, "误操作").unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");

    let error = echo::revoke_principle(&conn, "missing-node", "不存在").unwrap_err();
    assert_eq!(error.code(), "E_NOT_FOUND");
}
