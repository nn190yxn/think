//! 主动搜集的检索实现：把连接器的搜索结果翻译成待确认材料。
//!
//! 主动搜集默认关闭，开关在 `distill` 的设置里；这里只负责「已经允许联网且
//! 检索连接器已启用」时的实际取数，并把每次外发请求写入连接器调用审计。

use std::time::Instant;

use rusqlite::Connection;
use thought_forge_core::connector::repo::{self as connector_repo, ConnectorCallRecord};
use thought_forge_core::connector::{
    guard, normalize_snippet, KIND_SEARCH, PURPOSE_DISCOVERY, SearchProvider,
};
use thought_forge_core::cost;
use thought_forge_core::distill::intake::{DiscoveredMaterial, DiscoveryClient};
use thought_forge_core::error::{CoreError, CoreResult};

/// 联网关闭或检索连接器未启用时的占位实现。
///
/// 这里报 `E_NETWORK_OFF` 而不是静默返回空列表：主动搜集是用户显式触发的动作，
/// 拿不到结果时必须让人知道原因，而不是看到一份空的待确认清单。
pub struct OfflineDiscovery {
    pub message: String,
}

impl OfflineDiscovery {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl DiscoveryClient for OfflineDiscovery {
    fn search(&self, _query: &str) -> CoreResult<Vec<DiscoveredMaterial>> {
        Err(CoreError::NetworkOff(self.message.clone()))
    }
}

/// 用启用的检索连接器执行一次主动搜集。
pub struct ShellDiscovery<'a> {
    pub conn: &'a Connection,
    pub search: &'a dyn SearchProvider,
    pub max_results: usize,
    /// 发送模式，取连接器的 `connector.query_mode`。
    pub mode: String,
}

impl DiscoveryClient for ShellDiscovery<'_> {
    fn search(&self, query: &str) -> CoreResult<Vec<DiscoveredMaterial>> {
        let prepared = guard::prepare_query(self.conn, query, &self.mode)?;
        let connector_id = connector_repo::enabled_of_kind(self.conn, KIND_SEARCH)?.map(|v| v.id);
        let started = Instant::now();
        let outcome = self.search.search(&prepared.sent, self.max_results);
        let latency = started.elapsed().as_millis() as i64;

        match outcome {
            Ok(hits) => {
                let cost_micros = cost::record_connector_cost(self.conn, connector_id.as_deref())?;
                connector_repo::record_call(
                    self.conn,
                    &ConnectorCallRecord {
                        connector_id,
                        kind: KIND_SEARCH.to_string(),
                        purpose: PURPOSE_DISCOVERY.to_string(),
                        session_id: None,
                        query: prepared.original.clone(),
                        query_sent: prepared.sent.clone(),
                        redacted: prepared.redacted,
                        result_count: hits.len() as i64,
                        cost_micros,
                        latency_ms: latency,
                        status: "ok".to_string(),
                        error_code: None,
                    },
                )?;
                Ok(hits
                    .into_iter()
                    .take(self.max_results)
                    .filter(|hit| !hit.url.trim().is_empty())
                    .map(to_material)
                    .collect())
            }
            Err(error) => {
                connector_repo::record_call(
                    self.conn,
                    &ConnectorCallRecord {
                        connector_id,
                        kind: KIND_SEARCH.to_string(),
                        purpose: PURPOSE_DISCOVERY.to_string(),
                        session_id: None,
                        query: prepared.original,
                        query_sent: prepared.sent,
                        redacted: prepared.redacted,
                        result_count: 0,
                        cost_micros: 0,
                        latency_ms: latency,
                        status: "error".to_string(),
                        error_code: Some(error.code().to_string()),
                    },
                )?;
                Err(error)
            }
        }
    }
}

/// 外部标题与摘要在入库前统一过安检，命中可疑指令时由内核在预览里标注。
fn to_material(hit: thought_forge_core::connector::SearchHit) -> DiscoveredMaterial {
    let title = guard::sanitize_external(hit.title.trim());
    let summary = guard::sanitize_external(&normalize_snippet(&hit.snippet));
    DiscoveredMaterial {
        title: title.text,
        source_ref: hit.url.trim().to_string(),
        summary: summary.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thought_forge_core::connector::SearchHit;
    use thought_forge_core::db::{self, migrations};

    struct StubSearch {
        hits: Vec<SearchHit>,
        fail: bool,
    }

    impl SearchProvider for StubSearch {
        fn search(&self, _query: &str, _limit: usize) -> CoreResult<Vec<SearchHit>> {
            if self.fail {
                return Err(CoreError::MalformedResponse("上游返回不合约定".to_string()));
            }
            Ok(self.hits.clone())
        }
    }

    fn memory_db() -> Connection {
        let mut conn = db::open_in_memory().expect("建立内存库");
        migrations::apply_all(&mut conn).expect("迁移到位");
        conn
    }

    fn hit(title: &str, url: &str, snippet: &str) -> SearchHit {
        SearchHit {
            title: title.to_string(),
            url: url.to_string(),
            snippet: snippet.to_string(),
            published_at: None,
        }
    }

    fn client<'a>(conn: &'a Connection, search: &'a dyn SearchProvider) -> ShellDiscovery<'a> {
        ShellDiscovery {
            conn,
            search,
            max_results: 5,
            mode: guard::MODE_QUESTION.to_string(),
        }
    }

    #[test]
    fn maps_hits_to_materials_and_keeps_order() {
        let conn = memory_db();
        let search = StubSearch {
            hits: vec![
                hit("第一篇", "https://example.com/a", "摘要甲"),
                hit("第二篇", "https://example.com/b", "摘要乙"),
            ],
            fail: false,
        };
        let materials = client(&conn, &search).search("思想熔炉 采集").expect("检索成功");
        assert_eq!(materials.len(), 2);
        assert_eq!(materials[0].title, "第一篇");
        assert_eq!(materials[0].source_ref, "https://example.com/a");
        assert_eq!(materials[1].title, "第二篇");
    }

    #[test]
    fn drops_hits_without_url() {
        let conn = memory_db();
        let search = StubSearch {
            hits: vec![
                hit("有地址", "https://example.com/a", "摘要"),
                hit("无地址", "   ", "摘要"),
            ],
            fail: false,
        };
        let materials = client(&conn, &search).search("思想熔炉").expect("检索成功");
        assert_eq!(materials.len(), 1);
        assert_eq!(materials[0].source_ref, "https://example.com/a");
    }

    #[test]
    fn respects_result_limit() {
        let conn = memory_db();
        let search = StubSearch {
            hits: vec![
                hit("一", "https://example.com/1", ""),
                hit("二", "https://example.com/2", ""),
                hit("三", "https://example.com/3", ""),
            ],
            fail: false,
        };
        let mut narrow = client(&conn, &search);
        narrow.max_results = 2;
        assert_eq!(narrow.search("思想熔炉").expect("检索成功").len(), 2);
    }

    #[test]
    fn records_audit_row_for_success() {
        let conn = memory_db();
        let search = StubSearch {
            hits: vec![hit("一篇", "https://example.com/a", "摘要")],
            fail: false,
        };
        client(&conn, &search).search("思想熔炉 蒸馏").expect("检索成功");

        let calls = connector_repo::recent_calls(&conn, 10).expect("读审计");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].purpose, PURPOSE_DISCOVERY);
        assert_eq!(calls[0].status, "ok");
        assert_eq!(calls[0].result_count, 1);
        assert!(calls[0].query_sent.contains("思想熔炉"));
    }

    #[test]
    fn records_audit_row_for_failure_and_propagates() {
        let conn = memory_db();
        let search = StubSearch {
            hits: Vec::new(),
            fail: true,
        };
        let error = client(&conn, &search)
            .search("思想熔炉")
            .expect_err("上游失败应向上传递");
        assert_eq!(error.code(), "E_MALFORMED_RESPONSE");

        let calls = connector_repo::recent_calls(&conn, 10).expect("读审计");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].status, "error");
        assert_eq!(calls[0].error_code.as_deref(), Some("E_MALFORMED_RESPONSE"));
    }

    #[test]
    fn offline_client_reports_network_off() {
        let error = OfflineDiscovery::new("联网能力已关闭")
            .search("思想熔炉")
            .expect_err("未联网应报错");
        assert_eq!(error.code(), "E_NETWORK_OFF");
    }
}
