//! OpenAI-compatible 客户端。
//!
//! 请求组装与响应解析是纯函数，传输通过 [`HttpTransport`] 注入，因此可以
//! 在没有网络的环境里完整测试失败重试与解析分支。密钥不落库，由外壳从系统
//! 凭据读入并只在内存中传给客户端。

use serde_json::{json, Value};

use crate::error::{CoreError, CoreResult};

use super::{ModelClient, ModelRequest, ModelResponse};

/// HTTP 发送抽象。
pub trait HttpTransport {
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> CoreResult<String>;
}

/// 基于 OpenAI-compatible `/chat/completions` 的客户端。
pub struct OpenAiCompatible<T> {
    pub transport: T,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub platform: String,
}

impl<T: HttpTransport> OpenAiCompatible<T> {
    pub fn new(
        transport: T,
        endpoint: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        platform: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            endpoint: endpoint.into(),
            api_key: api_key.into(),
            model: model.into(),
            platform: platform.into(),
        }
    }
}

/// 校验 endpoint 必须是 http(s) 绝对地址。
pub fn validate_endpoint(endpoint: &str) -> CoreResult<()> {
    let trimmed = endpoint.trim();
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        return Ok(());
    }
    Err(CoreError::InvalidInput(format!(
        "模型平台地址必须是 http(s) 绝对地址：{endpoint}"
    )))
}

/// 组装请求体。
pub fn build_payload(request: &ModelRequest, model: &str, stream: bool) -> String {
    json!({
        "model": model,
        "messages": [
            { "role": "system", "content": request.system },
            { "role": "user", "content": request.user },
        ],
        "max_tokens": request.max_tokens,
        "temperature": 0.4,
        "stream": stream,
    })
    .to_string()
}

/// 解析非流式响应。
pub fn parse_completion(raw: &str) -> CoreResult<ModelResponse> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| CoreError::MalformedResponse(format!("响应不是合法 JSON：{error}")))?;

    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("模型平台返回错误");
        return Err(CoreError::ModelUnavailable {
            status: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: message.to_string(),
        });
    }

    let content = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::MalformedResponse("响应缺少 choices[0].message.content".into()))?;

    let usage = value.get("usage");
    Ok(ModelResponse {
        content: content.to_string(),
        platform: String::new(),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        prompt_tokens: usage
            .and_then(|usage| usage.get("prompt_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
        completion_tokens: usage
            .and_then(|usage| usage.get("completion_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    })
}

/// 解析流式响应中的一行，返回增量文本。
pub fn parse_sse_delta(line: &str) -> Option<String> {
    let data = line.trim().strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        return None;
    }
    let value: Value = serde_json::from_str(data).ok()?;
    value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .map(str::to_string)
}

impl<T: HttpTransport> ModelClient for OpenAiCompatible<T> {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse> {
        validate_endpoint(&self.endpoint)?;
        let body = build_payload(request, &self.model, false);
        let headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            (
                "Authorization".to_string(),
                format!("Bearer {}", self.api_key),
            ),
        ];
        let raw = self.transport.post_json(&self.endpoint, &headers, &body)?;
        let mut response = parse_completion(&raw)?;
        response.platform = self.platform.clone();
        if response.model.is_empty() {
            response.model = self.model.clone();
        }
        Ok(response)
    }
}
