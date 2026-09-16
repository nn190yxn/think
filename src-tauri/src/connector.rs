//! 桌面外壳的连接器装配。
//!
//! 内核只依赖 `SearchProvider`、`PageReader`、`ToolProvider` 抽象，真实调用在这里接入：
//! 搜索走 SearXNG 兼容的 JSON 接口，网页阅读走通用 HTML 正文提取，MCP 工具走
//! JSON-RPC 2.0 over Streamable HTTP。解析与正文提取拆成自由函数，便于离线单测；
//! 网络部分只负责发送，失败统一转成内核错误码，由编排层降级为「本次未获得外部背景」。
//!
//! 连接器默认关闭，且受全局联网开关约束：只有开关打开、且 `connectors` 表中
//! `enabled = 1`、`status = 'ready'` 的条目才会装配成真实能力。

use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use thought_forge_core::connector::repo;
use thought_forge_core::connector::service::Retrieval;
use thought_forge_core::connector::{
    normalize_snippet, PageContent, PageReader, SearchHit, SearchProvider, ToolProvider, ToolSpec,
    KIND_PAGE, KIND_SEARCH,
};
use thought_forge_core::credential::{ref_name_for, CredentialStore, SCOPE_CONNECTOR};
use thought_forge_core::{CoreError, CoreResult};

use crate::credential::ShellCredentialStore;

/// 单次连接器请求的超时。
const TIMEOUT_SECS: u64 = 30;
/// 响应体读取上限，避免超大页面占满内存。
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// MCP 客户端声明使用的协议版本。
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
/// 连接器密钥的环境变量回退名，与运行环境自身的变量名无关。
pub const CONNECTOR_KEY_ENV: &str = "THOUGHT_FORGE_CONNECTOR_KEY";

/// 外部调用发不出去或对端返回失败。
fn call_failed(message: impl Into<String>) -> CoreError {
    CoreError::NetworkOff(message.into())
}

/// 对端返回的结构不符合约定。
fn malformed(message: impl Into<String>) -> CoreError {
    CoreError::MalformedResponse(message.into())
}

fn http_client() -> CoreResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|error| call_failed(format!("初始化连接器传输失败：{error}")))
}

/// 读取响应体，超出上限的部分丢弃。
fn read_body(response: reqwest::blocking::Response) -> CoreResult<String> {
    let mut buffer = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES)
        .read_to_end(&mut buffer)
        .map_err(|error| call_failed(format!("读取连接器响应失败：{error}")))?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// 按连接器 id 解析密钥：先查系统凭据库，再退回环境变量；都没有时按无需密钥处理。
fn resolve_key(connector_id: Option<&str>) -> String {
    if let Some(id) = connector_id.filter(|id| !id.trim().is_empty()) {
        if let Ok(ref_name) = ref_name_for(SCOPE_CONNECTOR, id) {
            if let Ok(Some(secret)) = ShellCredentialStore::new().get(&ref_name) {
                if !secret.is_empty() {
                    return secret;
                }
            }
        }
    }
    std::env::var(CONNECTOR_KEY_ENV).unwrap_or_default()
}

/// SearXNG 兼容的检索地址：端点未带 `/search` 时补齐。
fn search_url(endpoint: &str, query: &str, limit: usize) -> String {
    let base = endpoint.trim().trim_end_matches('/');
    let base = if base.ends_with("/search") {
        base.to_string()
    } else {
        format!("{base}/search")
    };
    format!("{base}?q={}&format=json&limit={limit}", encode_query(query))
}

/// 查询串百分号编码，只保留 RFC 3986 的未保留字符。
fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 解析 SearXNG 的 JSON 检索结果，跳过没有网址的条目并按上限截断。
fn parse_searxng_results(body: &str, limit: usize) -> CoreResult<Vec<SearchHit>> {
    let root: Value = serde_json::from_str(body)
        .map_err(|error| malformed(format!("检索服务返回的不是 JSON：{error}")))?;
    let results = root
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed("检索服务返回中缺少 results 数组"))?;
    let mut hits = Vec::new();
    for item in results {
        let url = item.get("url").and_then(Value::as_str).unwrap_or("").trim();
        if url.is_empty() {
            continue;
        }
        hits.push(SearchHit {
            title: item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
            url: url.to_string(),
            snippet: normalize_snippet(
                item.get("content").and_then(Value::as_str).unwrap_or(""),
            ),
            published_at: item
                .get("publishedDate")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

/// 整块丢弃的噪音标签。
const DROP_TAGS: [&str; 3] = ["script", "style", "noscript"];
/// 结束位置换成换行的块级标签，用来保留段落边界。
const BLOCK_TAGS: [&str; 16] = [
    "p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article",
    "blockquote", "header", "footer",
];

/// 在 `from` 之后查找 ASCII 字节。
fn find_byte(haystack: &str, from: usize, byte: u8) -> Option<usize> {
    haystack.as_bytes()[from..]
        .iter()
        .position(|current| *current == byte)
        .map(|offset| from + offset)
}

/// ASCII 大小写无关地查找子串。命中位置必为字符边界，可直接切片。
fn find_ci(haystack: &str, from: usize, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() {
        return None;
    }
    (from..=bytes.len() - needle.len())
        .find(|index| bytes[*index..*index + needle.len()].eq_ignore_ascii_case(needle))
}

/// 解码常见 HTML 实体与数字引用。
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find('&') {
        out.push_str(&rest[..position]);
        let after = &rest[position + 1..];
        let candidate = after
            .find(';')
            .filter(|end| *end <= 12)
            .map(|end| &after[..end]);
        match candidate.and_then(decode_entity) {
            Some(decoded) => {
                out.push_str(&decoded);
                // 消耗实体名与 '&'、';' 两个定界符；无法识别时只跳过 '&'。
                let consumed = candidate.map(|name| name.len() + 2).unwrap_or(0);
                rest = &rest[position + consumed..];
            }
            None => {
                out.push('&');
                rest = &rest[position + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<String> {
    let named = match entity {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" | "#39" => "'",
        "nbsp" => " ",
        _ => {
            let code = entity.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse::<u32>().ok()?,
            };
            return char::from_u32(value).map(|ch| ch.to_string());
        }
    };
    Some(named.to_string())
}

/// HTML 正文提取：丢弃噪音标签与注释，块级标签换行，最后解码实体。
fn html_to_text(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'<' {
            let start = index;
            while index < bytes.len() && bytes[index] != b'<' {
                index += 1;
            }
            out.push_str(&html[start..index]);
            continue;
        }
        if html[index..].starts_with("<!--") {
            index = find_ci(html, index + 4, "-->")
                .map(|end| end + 3)
                .unwrap_or(bytes.len());
            continue;
        }
        let mut name_end = index + 1;
        let closing = bytes.get(name_end) == Some(&b'/');
        if closing {
            name_end += 1;
        }
        let name_start = name_end;
        while name_end < bytes.len() && bytes[name_end].is_ascii_alphanumeric() {
            name_end += 1;
        }
        let name = &html[name_start..name_end];
        let tag_end = find_byte(html, name_end, b'>')
            .map(|end| end + 1)
            .unwrap_or(bytes.len());
        if DROP_TAGS.iter().any(|drop| name.eq_ignore_ascii_case(drop)) {
            index = if closing {
                tag_end
            } else {
                find_ci(html, tag_end, &format!("</{name}"))
                    .and_then(|end| find_byte(html, end, b'>'))
                    .map(|end| end + 1)
                    .unwrap_or(bytes.len())
            };
            continue;
        }
        if BLOCK_TAGS.iter().any(|block| name.eq_ignore_ascii_case(block)) {
            out.push('\n');
        }
        index = tag_end;
    }
    decode_entities(&out)
}

/// 取 `<title>` 文本，缺失时返回空串。
fn page_title(html: &str) -> String {
    let Some(open) = find_ci(html, 0, "<title").and_then(|start| find_byte(html, start, b'>'))
    else {
        return String::new();
    };
    let Some(close) = find_ci(html, open, "</title") else {
        return String::new();
    };
    decode_entities(html[open + 1..close].trim())
}

/// 从 JSON-RPC 响应体里取出负载：直接是 JSON 时原样解析，SSE 分帧时取首个可解析帧。
fn jsonrpc_payload(body: &str) -> CoreResult<Value> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') {
        return serde_json::from_str(trimmed)
            .map_err(|error| malformed(format!("MCP 响应不是 JSON：{error}")));
    }
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.starts_with('{') {
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                return Ok(value);
            }
        }
    }
    Err(malformed("MCP 响应中没有可解析的 JSON-RPC 负载"))
}

fn tool_specs(payload: &Value) -> CoreResult<Vec<ToolSpec>> {
    let tools = payload
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .ok_or_else(|| malformed("MCP 响应缺少 result.tools"))?;
    Ok(tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(Value::as_str)?;
            if name.is_empty() {
                return None;
            }
            Some(ToolSpec {
                name: name.to_string(),
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                input_schema_json: tool
                    .get("inputSchema")
                    .map(Value::to_string)
                    .unwrap_or_else(|| "{}".to_string()),
            })
        })
        .collect())
}

fn tool_output(payload: &Value) -> CoreResult<String> {
    if let Some(error) = payload.get("error") {
        return Err(malformed(format!("MCP 调用返回错误：{error}")));
    }
    let result = payload
        .get("result")
        .ok_or_else(|| malformed("MCP 响应缺少 result"))?;
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        return Err(malformed(format!("MCP 工具执行失败：{text}")));
    }
    Ok(text)
}

/// SearXNG 兼容的检索实现。
pub struct HttpSearchProvider {
    client: reqwest::blocking::Client,
    endpoint: String,
    api_key: String,
}

impl HttpSearchProvider {
    pub fn new(endpoint: impl Into<String>, api_key: impl Into<String>) -> CoreResult<Self> {
        Ok(Self {
            client: http_client()?,
            endpoint: endpoint.into(),
            api_key: api_key.into(),
        })
    }
}

impl SearchProvider for HttpSearchProvider {
    fn search(&self, query: &str, limit: usize) -> CoreResult<Vec<SearchHit>> {
        let url = search_url(&self.endpoint, query, limit);
        let mut request = self.client.get(&url).header("Accept", "application/json");
        if !self.api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.api_key));
        }
        let response = request
            .send()
            .map_err(|error| call_failed(format!("检索请求失败：{error}")))?;
        let status = response.status();
        let body = read_body(response)?;
        if !status.is_success() {
            return Err(call_failed(format!(
                "检索服务返回 {status}：{}",
                normalize_snippet(&body)
            )));
        }
        parse_searxng_results(&body, limit)
    }
}

/// 通用网页阅读实现：抓取地址并按 HTML 正文提取。
pub struct HttpPageReader {
    client: reqwest::blocking::Client,
}

impl HttpPageReader {
    pub fn new() -> CoreResult<Self> {
        Ok(Self {
            client: http_client()?,
        })
    }
}

impl PageReader for HttpPageReader {
    fn read(&self, url: &str) -> CoreResult<PageContent> {
        let response = self
            .client
            .get(url)
            .header("Accept", "text/html,application/xhtml+xml")
            .send()
            .map_err(|error| call_failed(format!("网页抓取失败：{error}")))?;
        let status = response.status();
        let body = read_body(response)?;
        if !status.is_success() {
            return Err(call_failed(format!("网页返回 {status}")));
        }
        Ok(PageContent {
            title: page_title(&body),
            text: html_to_text(&body),
        })
    }
}

/// MCP 工具实现：JSON-RPC 2.0 走 Streamable HTTP，握手后复用会话标识。
pub struct HttpToolProvider {
    client: reqwest::blocking::Client,
    endpoint: String,
    api_key: String,
    session: Mutex<Option<String>>,
    initialized: Mutex<bool>,
}

impl HttpToolProvider {
    pub fn new(endpoint: impl Into<String>, api_key: impl Into<String>) -> CoreResult<Self> {
        Ok(Self {
            client: http_client()?,
            endpoint: endpoint.into(),
            api_key: api_key.into(),
            session: Mutex::new(None),
            initialized: Mutex::new(false),
        })
    }

    /// 发送一次 JSON-RPC 请求，返回状态码与响应体，并记录会话标识。
    fn post(
        &self,
        body: String,
    ) -> CoreResult<(reqwest::StatusCode, String)> {
        let mut request = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION);
        if !self.api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.api_key));
        }
        if let Some(session) = self.session.lock().ok().and_then(|guard| guard.clone()) {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request
            .body(body)
            .send()
            .map_err(|error| call_failed(format!("MCP 请求失败：{error}")))?;
        let status = response.status();
        if let Some(session) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
        {
            if let Ok(mut guard) = self.session.lock() {
                *guard = Some(session.to_string());
            }
        }
        let text = read_body(response)?;
        if !status.is_success() {
            return Err(call_failed(format!(
                "MCP 服务器返回 {status}：{}",
                normalize_snippet(&text)
            )));
        }
        Ok((status, text))
    }

    fn rpc(&self, id: u64, method: &str, params: Value) -> CoreResult<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
        .to_string();
        let (_, text) = self.post(body)?;
        jsonrpc_payload(&text)
    }

    /// 是否已完成握手，避免每次调用都重新初始化。
    fn is_initialized(&self) -> bool {
        self.initialized
            .lock()
            .map(|guard| *guard)
            .unwrap_or(false)
    }

    /// 握手：`initialize` 后按协议补发 `initialized` 通知。
    fn initialize(&self) -> CoreResult<()> {
        if self.is_initialized() {
            return Ok(());
        }
        let payload = self.rpc(
            1,
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "thought-forge",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        )?;
        if payload.get("result").is_none() {
            return Err(malformed("MCP 初始化未返回结果"));
        }
        // 通知没有 id，也没有响应体；失败只表示对端不认这条通知，不影响后续调用。
        let notified = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        })
        .to_string();
        let _ = self.post(notified);
        if let Ok(mut guard) = self.initialized.lock() {
            *guard = true;
        }
        Ok(())
    }
}

impl ToolProvider for HttpToolProvider {
    fn list_tools(&self) -> CoreResult<Vec<ToolSpec>> {
        self.initialize()?;
        let payload = self.rpc(2, "tools/list", json!({}))?;
        tool_specs(&payload)
    }

    fn call_tool(&self, name: &str, arguments_json: &str) -> CoreResult<String> {
        let arguments: Value = if arguments_json.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(arguments_json)
                .map_err(|error| malformed(format!("工具参数不是合法 JSON：{error}")))?
        };
        self.initialize()?;
        let payload = self.rpc(
            3,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        tool_output(&payload)
    }
}

/// 一次会诊可用的外部能力集合。全局联网关闭或对应连接器未启用时留空。
pub struct ShellConnector {
    search: Option<HttpSearchProvider>,
    page: Option<HttpPageReader>,
}

impl ShellConnector {
    pub fn from_db(conn: &rusqlite::Connection, networking_enabled: bool) -> CoreResult<Self> {
        if !networking_enabled {
            return Ok(Self {
                search: None,
                page: None,
            });
        }
        let search = match repo::enabled_of_kind(conn, KIND_SEARCH)? {
            Some(view) => Some(HttpSearchProvider::new(
                &view.endpoint,
                resolve_key(Some(&view.id)),
            )?),
            None => None,
        };
        let page = match repo::enabled_of_kind(conn, KIND_PAGE)? {
            Some(_) => Some(HttpPageReader::new()?),
            None => None,
        };
        Ok(Self { search, page })
    }

    pub fn retrieval(&self) -> Retrieval<'_> {
        Retrieval {
            search: self
                .search
                .as_ref()
                .map(|provider| provider as &dyn SearchProvider),
            page: self
                .page
                .as_ref()
                .map(|provider| provider as &dyn PageReader),
        }
    }
}

/// 按配置装配一个检索连接器，供配置预检与连通测试使用。
pub fn search_provider(
    endpoint: &str,
    connector_id: Option<&str>,
) -> CoreResult<HttpSearchProvider> {
    HttpSearchProvider::new(endpoint, resolve_key(connector_id))
}

/// 按配置装配一个网页阅读器。
pub fn page_reader() -> CoreResult<HttpPageReader> {
    HttpPageReader::new()
}

/// 按配置装配一个 MCP 工具客户端。
pub fn tool_provider(
    endpoint: &str,
    connector_id: Option<&str>,
) -> CoreResult<HttpToolProvider> {
    HttpToolProvider::new(endpoint, resolve_key(connector_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_encoding_keeps_unreserved_and_escapes_rest() {
        assert_eq!(encode_query("abc-._~"), "abc-._~");
        assert_eq!(encode_query("a b"), "a%20b");
        assert_eq!(encode_query("歧义&x=1"), "%E6%AD%A7%E4%B9%89%26x%3D1");
    }

    #[test]
    fn search_url_appends_search_path_once() {
        assert_eq!(
            search_url("http://localhost:8080", "a b", 5),
            "http://localhost:8080/search?q=a%20b&format=json&limit=5"
        );
        // 用户直接填了 /search 时不再重复追加。
        assert_eq!(
            search_url("http://localhost:8080/search/", "q", 1),
            "http://localhost:8080/search?q=q&format=json&limit=1"
        );
    }

    #[test]
    fn searxng_results_are_parsed_and_truncated() {
        let body = r#"{"results":[
            {"title":"一","url":"https://a.example/1","content":"摘要 一","publishedDate":"2026-09-15T00:00:00Z"},
            {"title":"二","url":"https://a.example/2","content":"摘要 二"},
            {"title":"无网址","url":"","content":"忽略"}
        ]}"#;
        let hits = parse_searxng_results(body, 5).expect("解析成功");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "一");
        assert_eq!(hits[0].url, "https://a.example/1");
        assert_eq!(hits[0].published_at.as_deref(), Some("2026-09-15T00:00:00Z"));
        assert!(hits[1].published_at.is_none());
        let truncated = parse_searxng_results(body, 1).expect("解析成功");
        assert_eq!(truncated.len(), 1);
    }

    #[test]
    fn searxng_missing_results_is_malformed() {
        let error = parse_searxng_results("{\"answers\":[]}", 3).unwrap_err();
        assert_eq!(error.code(), "E_MALFORMED_RESPONSE");
        let error = parse_searxng_results("<html>403</html>", 3).unwrap_err();
        assert_eq!(error.code(), "E_MALFORMED_RESPONSE");
    }

    #[test]
    fn html_text_drops_noise_and_keeps_paragraphs() {
        let html = "<html><head><title>标题 &amp; 副题</title>\
                    <style>body{color:red}</style></head>\
                    <body><script>var a = 1 < 2;</script>\
                    <p>第一段 &lt;保留&gt;</p><p>第二段&nbsp;结尾</p>\
                    <!-- 注释 --></body></html>";
        assert_eq!(page_title(html), "标题 & 副题");
        let text = html_to_text(html);
        assert!(text.contains("第一段 <保留>"));
        assert!(text.contains("第二段 结尾"));
        assert!(!text.contains("var a"));
        assert!(!text.contains("color:red"));
        assert!(!text.contains("注释"));
        // 标签已剥离；实体解码后出现的 `<保留>` 属于正文，不算残留标签。
        assert!(!text.contains("<p>"));
        assert!(!text.contains("</p>"));
    }

    #[test]
    fn html_text_decodes_numeric_entities_and_keeps_unknown_ampersand() {
        assert_eq!(html_to_text("&#65;&#x42; R&D"), "AB R&D");
        assert_eq!(html_to_text("A & B"), "A & B");
    }

    #[test]
    fn jsonrpc_payload_accepts_plain_and_sse_frames() {
        let plain = jsonrpc_payload(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#).expect("解析成功");
        assert_eq!(plain["id"], 1);
        let sse = jsonrpc_payload("event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}\n\n")
            .expect("解析成功");
        assert_eq!(sse["id"], 2);
        assert_eq!(
            jsonrpc_payload("event: ping\n\n").unwrap_err().code(),
            "E_MALFORMED_RESPONSE"
        );
    }

    #[test]
    fn tool_specs_require_name_and_keep_schema() {
        let payload = json!({"result": {"tools": [
            {"name": "search", "description": "检索", "inputSchema": {"type": "object"}},
            {"description": "缺少名字"}
        ]}});
        let specs = tool_specs(&payload).expect("解析成功");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "search");
        assert!(specs[0].input_schema_json.contains("\"type\""));
        assert!(tool_specs(&json!({"result": {}})).is_err());
    }

    #[test]
    fn tool_output_rejects_error_payloads() {
        let ok = json!({"result": {"content": [{"type": "text", "text": "结果"}]}});
        assert_eq!(tool_output(&ok).expect("解析成功"), "结果");
        let failed = json!({"result": {"isError": true, "content": [{"text": "越界"}]}});
        assert_eq!(tool_output(&failed).unwrap_err().code(), "E_MALFORMED_RESPONSE");
        let rpc_error = json!({"error": {"code": -32601, "message": "方法不存在"}});
        assert!(tool_output(&rpc_error).unwrap_err().to_string().contains("方法不存在"));
    }
}
