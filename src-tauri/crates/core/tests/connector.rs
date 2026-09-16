//! 连接器：检索隔离、共享背景一致、快照冻结、上限约束、正文快照与失败降级。

use std::cell::RefCell;

use thought_forge_core::connector::{
    self as connector, repo as connector_repo, service as connector_service,
    service::Retrieval, PageContent, PageReader, SearchHit, SearchProvider, ToolProvider, ToolSpec,
};
use thought_forge_core::council::{repo, tuning, Strategy};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::{CoreError, CoreResult};

struct ScriptedSearch {
    calls: RefCell<usize>,
    queries: RefCell<Vec<String>>,
    hits: Vec<SearchHit>,
    fail: bool,
}

impl ScriptedSearch {
    fn new(count: usize) -> Self {
        let hits = (0..count)
            .map(|index| SearchHit {
                title: format!("外部资料{index}"),
                url: format!("https://example.com/{index}"),
                snippet: format!("这是第 {index} 条外部资料的内容"),
                published_at: Some("2026-06-01T00:00:00Z".to_string()),
            })
            .collect();
        Self {
            calls: RefCell::new(0),
            queries: RefCell::new(Vec::new()),
            hits,
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            calls: RefCell::new(0),
            queries: RefCell::new(Vec::new()),
            hits: Vec::new(),
            fail: true,
        }
    }

    fn call_count(&self) -> usize {
        *self.calls.borrow()
    }
}

impl SearchProvider for ScriptedSearch {
    fn search(&self, query: &str, limit: usize) -> CoreResult<Vec<SearchHit>> {
        *self.calls.borrow_mut() += 1;
        self.queries.borrow_mut().push(query.to_string());
        if self.fail {
            return Err(CoreError::NetworkOff("脚本化检索失败".to_string()));
        }
        Ok(self.hits.iter().take(limit).cloned().collect())
    }
}

struct ScriptedPage {
    calls: RefCell<usize>,
}

impl PageReader for ScriptedPage {
    fn read(&self, _url: &str) -> CoreResult<PageContent> {
        *self.calls.borrow_mut() += 1;
        Ok(PageContent {
            title: "正文标题".to_string(),
            text: "正".repeat(connector::MAX_BODY_CHARS + 100),
        })
    }
}

struct ScriptedTools {
    specs: Vec<ToolSpec>,
}

impl ToolProvider for ScriptedTools {
    fn list_tools(&self) -> CoreResult<Vec<ToolSpec>> {
        Ok(self.specs.clone())
    }

    fn call_tool(&self, _name: &str, _arguments_json: &str) -> CoreResult<String> {
        Ok("ok".to_string())
    }
}

fn memory_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn session(conn: &rusqlite::Connection) -> String {
    repo::create_session(conn, "要不要换一条路走", &[], &[], Strategy::Steady).expect("会诊可新建")
}

fn set_tuning(conn: &rusqlite::Connection, key: &str, value: &str) {
    tuning::set(conn, &[(key.to_string(), value.to_string())]).expect("调参可写入");
}

#[test]
fn background_snapshot_is_frozen_and_audited() {
    let conn = memory_db();
    let session_id = session(&conn);
    set_tuning(&conn, "connector.max_results", "3");
    let search = ScriptedSearch::new(10);
    let retrieval = Retrieval {
        search: Some(&search),
        page: None,
    };

    let first =
        connector_service::collect_background(&conn, &retrieval, &session_id, 0, "换赛道值不值")
            .expect("共享背景可检索");
    assert_eq!(first.len(), 3, "结果数按上限截断");
    assert!(first.iter().all(|source| source.master_id.is_none()));

    let second =
        connector_service::collect_background(&conn, &retrieval, &session_id, 0, "换赛道值不值")
            .expect("二次读取可复用快照");
    assert_eq!(second.len(), 3);
    assert_eq!(search.call_count(), 1, "快照冻结后不再重复检索");

    let calls = connector_repo::recent_calls(&conn, 10).expect("可读审计");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].purpose, connector::PURPOSE_BACKGROUND);
    assert_eq!(calls[0].result_count, 3);
    assert_eq!(calls[0].status, "ok");
}

#[test]
fn max_searches_per_session_is_respected() {
    let conn = memory_db();
    let session_id = session(&conn);
    set_tuning(&conn, "connector.max_searches_per_session", "1");
    let search = ScriptedSearch::new(2);
    let retrieval = Retrieval {
        search: Some(&search),
        page: None,
    };

    connector_service::collect_background(&conn, &retrieval, &session_id, 0, "问题")
        .expect("首次检索可用");
    let seat = connector_service::collect_for_seat(
        &conn,
        &retrieval,
        &session_id,
        0,
        1,
        "m-1",
        &["席位问句".to_string()],
    )
    .expect("超限时安全返回");

    assert!(seat.is_empty(), "达到上限后不再检索");
    assert_eq!(search.call_count(), 1, "总检索次数不超过上限");
    let count = connector_repo::search_count(&conn, &session_id).expect("可统计");
    assert_eq!(count, 1);
}

#[test]
fn seat_search_stays_within_its_own_seat() {
    let conn = memory_db();
    let session_id = session(&conn);
    set_tuning(&conn, "council.seat_search", "true");
    let search = ScriptedSearch::new(2);
    let retrieval = Retrieval {
        search: Some(&search),
        page: None,
    };

    let seat = connector_service::collect_for_seat(
        &conn,
        &retrieval,
        &session_id,
        0,
        1,
        "m-1",
        &["m-1 的问句".to_string()],
    )
    .expect("席位检索可用");
    assert_eq!(seat.len(), 2);
    assert!(seat.iter().all(|source| source.master_id.as_deref() == Some("m-1")));

    let other = connector_service::seat_sources(&conn, &session_id, 0, "m-2").expect("可读");
    assert!(other.is_empty(), "其他席位看不到该席位的补充检索");
    let background =
        connector_service::background_sources(&conn, &session_id, 0).expect("可读共享背景");
    assert!(background.is_empty(), "席位补充检索不进入共享背景");

    let calls = connector_repo::recent_calls(&conn, 10).expect("可读审计");
    assert_eq!(calls[0].purpose, connector::PURPOSE_SEAT_SEARCH);
}

#[test]
fn body_snapshot_follows_the_toggle() {
    let conn = memory_db();
    let session_id = session(&conn);
    let search = ScriptedSearch::new(1);
    let page = ScriptedPage {
        calls: RefCell::new(0),
    };
    let retrieval = Retrieval {
        search: Some(&search),
        page: Some(&page),
    };

    let without =
        connector_service::collect_background(&conn, &retrieval, &session_id, 0, "问题")
            .expect("可检索");
    assert!(!without[0].has_body, "默认不保存正文");
    assert_eq!(*page.calls.borrow(), 0);

    set_tuning(&conn, "connector.snapshot_body", "true");
    let session_id = session(&conn);
    let with_body =
        connector_service::collect_background(&conn, &retrieval, &session_id, 0, "问题")
            .expect("可检索");
    assert!(with_body[0].has_body, "开启后保存正文");
    let body = connector_service::body(&conn, &with_body[0].id)
        .expect("可读正文")
        .expect("正文存在");
    assert_eq!(body.chars().count(), connector::MAX_BODY_CHARS, "正文按上限截断");
}

#[test]
fn retrieval_failure_degrades_without_blocking() {
    let conn = memory_db();
    let session_id = session(&conn);
    let search = ScriptedSearch::failing();
    let retrieval = Retrieval {
        search: Some(&search),
        page: None,
    };

    let sources = connector_service::collect_background(&conn, &retrieval, &session_id, 0, "问题")
        .expect("检索失败不应中断会诊");
    assert!(sources.is_empty());
    let calls = connector_repo::recent_calls(&conn, 10).expect("可读审计");
    assert_eq!(calls[0].status, "failed");
    assert_eq!(calls[0].error_code.as_deref(), Some("E_NETWORK_OFF"));
}

#[test]
fn mcp_requires_tool_declarations() {
    let empty = connector::NoopToolProvider;
    assert!(connector::validate_tools(&empty).is_err(), "无工具声明应被拒绝");

    let declared = ScriptedTools {
        specs: vec![ToolSpec {
            name: "search".to_string(),
            description: "检索".to_string(),
            input_schema_json: "{}".to_string(),
        }],
    };
    let tools = connector::validate_tools(&declared).expect("有工具声明可接入");
    assert_eq!(tools.len(), 1);
}

#[test]
fn sources_block_marks_time_label_and_numbering() {
    let block = connector_service::sources_block(
        "共享背景",
        &[thought_forge_core::council::SourceView {
            id: "s-1".to_string(),
            kind: "search".to_string(),
            title: "标题".to_string(),
            url: "https://example.com/a".to_string(),
            snippet: "摘要".to_string(),
            published_at: Some("2026-06-01T00:00:00Z".to_string()),
            fetched_at: "2026-09-15T04:20:00Z".to_string(),
            has_body: false,
            flagged: false,
            master_id: None,
            round: 0,
        }],
        "2026-09-15T04:20:00Z",
    );
    assert!(block.contains("2026-09-15T04:20:00Z"));
    assert!(block.contains("共享背景"));
    assert!(block.contains("据资料"));
    assert!(block.contains("[1] 标题"));
}

#[test]
fn connector_status_follows_configuration() {
    let conn = memory_db();
    let saved = connector_repo::upsert(
        &conn,
        &connector::ConnectorInput {
            kind: connector::KIND_SEARCH.to_string(),
            display_name: "自建搜索".to_string(),
            endpoint: "http://127.0.0.1:8080/search".to_string(),
            ..Default::default()
        },
    )
    .expect("连接器可写入");
    assert!(!saved.enabled);
    assert_eq!(saved.status, "disabled");

    let enabled = connector_repo::set_enabled(&conn, &saved.id, true).expect("可启用");
    assert_eq!(enabled.status, "ready");
    let found = connector_repo::enabled_of_kind(&conn, connector::KIND_SEARCH)
        .expect("可查询")
        .expect("应能查到");
    assert_eq!(found.id, saved.id);
}
