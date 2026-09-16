//! 连接器：外部检索、网页阅读与 MCP 工具三类能力的统一抽象。
//!
//! 内核只定义接口、编排与审计，真实调用由桌面外壳注入，与既有的
//! `CaptureSource`、`DiscoveryClient`、`HttpTransport` 保持同一模式。

pub mod guard;
pub mod repo;
pub mod service;

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;

pub const KIND_SEARCH: &str = "search";
pub const KIND_PAGE: &str = "page";
pub const KIND_MCP: &str = "mcp";

/// 三类连接器，顺序与界面一致。
pub const CONNECTOR_KINDS: [&str; 3] = [KIND_SEARCH, KIND_PAGE, KIND_MCP];

/// 调用审计里的用途标识。
pub const PURPOSE_BACKGROUND: &str = "council_background";
pub const PURPOSE_SEAT_SEARCH: &str = "council_seat_search";
pub const PURPOSE_TEST: &str = "connector_test";

/// 网页正文快照的长度上限，超出截断。
pub const MAX_BODY_CHARS: usize = 20_000;
/// 单条摘要的长度上限。
pub const MAX_SNIPPET_CHARS: usize = 600;

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        KIND_SEARCH => "搜索",
        KIND_PAGE => "网页阅读",
        KIND_MCP => "MCP 工具",
        _ => "未知",
    }
}

pub fn is_known_kind(kind: &str) -> bool {
    CONNECTOR_KINDS.contains(&kind)
}

/// 连接器状态由「是否启用」与「配置是否完整」共同决定。
pub fn status_of(enabled: bool, endpoint: &str) -> &'static str {
    if endpoint.trim().is_empty() {
        "unconfigured"
    } else if enabled {
        "ready"
    } else {
        "disabled"
    }
}

/// 一个连接器的配置视图。密钥不入库，`config` 只放非敏感参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorView {
    pub id: String,
    pub kind: String,
    pub kind_label: String,
    pub display_name: String,
    pub endpoint: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 写入连接器配置的输入。
#[derive(Debug, Clone, Default)]
pub struct ConnectorInput {
    pub id: Option<String>,
    pub kind: String,
    pub display_name: String,
    pub endpoint: String,
    pub config: serde_json::Value,
}

/// 一次搜索命中的一条结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
}

/// 网页正文。
#[derive(Debug, Clone)]
pub struct PageContent {
    pub title: String,
    pub text: String,
}

/// MCP 服务器声明的工具能力。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
}

/// 搜索能力。外壳实现真实检索，测试用脚本化实现。
pub trait SearchProvider {
    fn search(&self, query: &str, limit: usize) -> CoreResult<Vec<SearchHit>>;
}

/// 网页阅读能力。
pub trait PageReader {
    fn read(&self, url: &str) -> CoreResult<PageContent>;
}

/// MCP 工具能力。只接受能返回工具能力声明的服务器。
pub trait ToolProvider {
    fn list_tools(&self) -> CoreResult<Vec<ToolSpec>>;
    fn call_tool(&self, name: &str, arguments_json: &str) -> CoreResult<String>;
}

/// 未接入检索源时的占位实现，返回空结果而非报错，便于离线自检。
pub struct NoopSearchProvider;

impl SearchProvider for NoopSearchProvider {
    fn search(&self, _query: &str, _limit: usize) -> CoreResult<Vec<SearchHit>> {
        Ok(Vec::new())
    }
}

/// 未接入网页阅读时的占位实现。
pub struct NoopPageReader;

impl PageReader for NoopPageReader {
    fn read(&self, _url: &str) -> CoreResult<PageContent> {
        Ok(PageContent {
            title: String::new(),
            text: String::new(),
        })
    }
}

/// 未接入 MCP 时的占位实现。空工具列表会被上层判为不合格服务器。
pub struct NoopToolProvider;

impl ToolProvider for NoopToolProvider {
    fn list_tools(&self) -> CoreResult<Vec<ToolSpec>> {
        Ok(Vec::new())
    }

    fn call_tool(&self, _name: &str, _arguments_json: &str) -> CoreResult<String> {
        Err(crate::error::CoreError::NetworkOff(
            "尚未接入 MCP 工具服务器".to_string(),
        ))
    }
}

/// 校验 MCP 服务器确实提供了工具能力声明，否则拒绝接入。
pub fn validate_tools(provider: &dyn ToolProvider) -> CoreResult<Vec<ToolSpec>> {
    let tools = provider.list_tools()?;
    if tools.is_empty() {
        return Err(crate::error::CoreError::InvalidInput(
            "该 MCP 服务器未声明任何工具，拒绝接入".to_string(),
        ));
    }
    Ok(tools)
}

/// 摘要归一化：折叠空白并按上限截断。
pub fn normalize_snippet(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_SNIPPET_CHARS {
        return collapsed;
    }
    collapsed.chars().take(MAX_SNIPPET_CHARS).collect()
}

/// 正文归一化：折叠空白并按上限截断。
pub fn normalize_body(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_BODY_CHARS {
        return collapsed;
    }
    collapsed.chars().take(MAX_BODY_CHARS).collect()
}
