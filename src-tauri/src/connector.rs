//! 桌面外壳的连接器装配。
//!
//! 内核只依赖 `SearchProvider`、`PageReader`、`ToolProvider` 抽象；真实搜索、
//! 网页抓取与 MCP 客户端应在这里注入。当前环境未接入外部服务，保留占位实现，
//! 使整条检索链路（审计、快照、降级）可以离线跑通。

use thought_forge_core::connector::service::Retrieval;
use thought_forge_core::connector::{PageContent, PageReader, SearchHit, SearchProvider};
use thought_forge_core::CoreResult;

/// 占位搜索连接器。接入真实服务时把 `search` 换成 HTTP 实现。
pub struct PlaceholderSearch {
    _endpoint: String,
}

impl PlaceholderSearch {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            _endpoint: endpoint.into(),
        }
    }
}

impl SearchProvider for PlaceholderSearch {
    fn search(&self, _query: &str, _limit: usize) -> CoreResult<Vec<SearchHit>> {
        // 未接入真实检索服务，按「本次未获得外部背景」处理。
        Ok(Vec::new())
    }
}

/// 占位网页阅读器。
pub struct PlaceholderPage {
    _endpoint: String,
}

impl PlaceholderPage {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            _endpoint: endpoint.into(),
        }
    }
}

impl PageReader for PlaceholderPage {
    fn read(&self, _url: &str) -> CoreResult<PageContent> {
        Ok(PageContent {
            title: String::new(),
            text: String::new(),
        })
    }
}

/// 一次会诊可用的外部能力集合。接入真实实现时替换这里的字段即可。
pub struct ShellConnector {
    search: PlaceholderSearch,
    page: PlaceholderPage,
}

impl ShellConnector {
    pub fn new() -> Self {
        Self {
            search: PlaceholderSearch::new(""),
            page: PlaceholderPage::new(""),
        }
    }

    pub fn retrieval(&self) -> Retrieval<'_> {
        Retrieval {
            search: Some(&self.search),
            page: Some(&self.page),
        }
    }
}

impl Default for ShellConnector {
    fn default() -> Self {
        Self::new()
    }
}
