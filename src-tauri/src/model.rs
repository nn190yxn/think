//! 桌面外壳的模型客户端装配。
//!
//! 纯逻辑 crate 只依赖 `ModelClient` 抽象，真实网络传输在这里接入。
//! 密钥不入库：只从用户自己的环境变量读取，缺失时按本地模型（无需密钥）处理。

use std::time::Duration;

use thought_forge_core::llm::openai::{HttpTransport, OpenAiCompatible};
use thought_forge_core::llm::platform::PlatformView;
use thought_forge_core::llm::{ModelClient, ModelRequest, ModelResponse};
use thought_forge_core::{CoreError, CoreResult};

/// 用户提供密钥用的环境变量名，与运行环境自身的变量名无关。
pub const API_KEY_ENV: &str = "THOUGHT_FORGE_API_KEY";

/// 基于 reqwest 的阻塞传输。响应体交给纯逻辑层解析，这里只负责发送。
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestTransport {
    pub fn new(timeout_secs: u64) -> CoreResult<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|error| CoreError::ModelUnavailable {
                status: 0,
                message: format!("初始化模型传输失败：{error}"),
            })?;
        Ok(Self { client })
    }
}

impl HttpTransport for ReqwestTransport {
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> CoreResult<String> {
        let mut request = self.client.post(url).body(body.to_string());
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.send().map_err(|error| CoreError::ModelUnavailable {
            status: error.status().map(|status| status.as_u16() as i64).unwrap_or(0),
            message: format!("模型平台请求失败：{error}"),
        })?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if !status.is_success() {
            return Err(CoreError::ModelUnavailable {
                status: status.as_u16() as i64,
                message: text,
            });
        }
        Ok(text)
    }
}

/// 按平台配置装配 OpenAI-compatible 客户端。
pub fn build_client(platform: &PlatformView) -> CoreResult<OpenAiCompatible<ReqwestTransport>> {
    let transport = ReqwestTransport::new(60)?;
    // 密钥优先取自系统凭据库，缺失时退回环境变量。
    let api_key = crate::credential::resolve_api_key(&platform.code);
    Ok(OpenAiCompatible::new(
        transport,
        platform.endpoint.clone(),
        api_key,
        platform.model_name.clone(),
        platform.code.clone(),
    ))
}

/// 占位客户端：把「联网关闭」或「平台未配置」变成可审计的失败调用，
/// 让编排链路照常写入轮次记录，而不是在命令层直接短路。
pub struct BlockedClient {
    network_off: bool,
    message: String,
}

impl BlockedClient {
    pub fn network_off(message: impl Into<String>) -> Self {
        Self {
            network_off: true,
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            network_off: false,
            message: message.into(),
        }
    }
}

impl ModelClient for BlockedClient {
    fn complete(&self, _request: &ModelRequest) -> CoreResult<ModelResponse> {
        if self.network_off {
            Err(CoreError::NetworkOff(self.message.clone()))
        } else {
            Err(CoreError::ModelUnavailable {
                status: 0,
                message: self.message.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ModelRequest {
        ModelRequest::new("model_probe", "系统提示", "只回复两个字：可用")
    }

    #[test]
    fn blocked_client_reports_network_off() {
        let client = BlockedClient::network_off("联网能力未开启");
        let error = client.complete(&request()).unwrap_err();
        assert_eq!(error.code(), "E_NETWORK_OFF");
        assert!(error.to_string().contains("联网能力未开启"));
    }

    #[test]
    fn blocked_client_reports_model_unavailable() {
        let client = BlockedClient::unavailable("平台未配置");
        let error = client.complete(&request()).unwrap_err();
        assert_eq!(error.code(), "E_MODEL_UNAVAILABLE");
        assert!(error.to_string().contains("平台未配置"));
    }
}
