//! 运行控制：取消只在运行中生效、心跳刷新、中断会话识别与断点续跑。

use thought_forge_core::council::{control, repo, Strategy};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::master::LAYER_ORDER;

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn session(conn: &rusqlite::Connection, question: &str) -> String {
    repo::create_session(conn, question, &[], &LAYER_ORDER, Strategy::Steady).expect("可建会话")
}

#[test]
fn cancel_takes_effect_only_while_running() {
    let conn = db();
    let id = session(&conn, "这次要不要停下");

    control::request_cancel(&conn, &id).expect("可请求取消");
    assert!(control::cancel_requested(&conn, &id).unwrap());

    // 已结束的会话请求取消按幂等处理，不改变状态。
    repo::update_status(&conn, &id, "done").unwrap();
    control::request_cancel(&conn, &id).expect("已结束会话按幂等处理");
    assert_eq!(repo::get_session(&conn, &id).unwrap().status, "done");

    let error = control::request_cancel(&conn, "missing").unwrap_err();
    assert_eq!(error.code(), "E_NOT_FOUND");
}

#[test]
fn heartbeat_and_recoverable_identify_stale_running_sessions() {
    let conn = db();
    let stale = session(&conn, "中断的会话");
    repo::update_status(&conn, &stale, "running").unwrap();
    conn.execute(
        "UPDATE council_sessions
         SET heartbeat_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-1 hour')
         WHERE id = ?1",
        [&stale],
    )
    .unwrap();

    let fresh = session(&conn, "刚起步的会话");
    repo::update_status(&conn, &fresh, "running").unwrap();
    control::heartbeat(&conn, &fresh).expect("可刷新心跳");

    let done = session(&conn, "已完成的会话");
    repo::update_status(&conn, &done, "done").unwrap();

    let ids: Vec<String> = control::recoverable(&conn, 120)
        .unwrap()
        .into_iter()
        .map(|view| view.id)
        .collect();
    assert!(ids.contains(&stale), "心跳过久的运行中会话应被识别");
    assert!(!ids.contains(&fresh), "刚心跳过的会话不算中断");
    assert!(!ids.contains(&done), "已结束会话不在恢复列表");
}
