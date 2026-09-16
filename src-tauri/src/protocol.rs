use serde::{Serialize, Serializer};

use thought_forge_core::{CoreError, CoreResult};

/// 固定为 true 的标记字段，避免把布尔字面量散落在业务结构里。
pub struct TrueFlag;
pub struct FalseFlag;

impl Serialize for TrueFlag {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(true)
    }
}

impl Serialize for FalseFlag {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(false)
    }
}

#[derive(Serialize)]
pub struct CommandSuccess<T> {
    pub ok: TrueFlag,
    pub data: T,
}

#[derive(Serialize)]
pub struct CommandFailure {
    pub ok: FalseFlag,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// command 边界的统一返回形状，与前端 `CommandResult` 对应。
#[derive(Serialize)]
#[serde(untagged)]
pub enum CommandResult<T> {
    Success(CommandSuccess<T>),
    Failure(CommandFailure),
}

impl<T> CommandResult<T> {
    pub fn ok(data: T) -> Self {
        CommandResult::Success(CommandSuccess {
            ok: TrueFlag,
            data,
        })
    }

    /// 命令层自行发现的错误（如参数无法解析）不必构造成 `CoreError`。
    pub fn failure(code: &'static str, message: impl Into<String>) -> Self {
        CommandResult::Failure(CommandFailure {
            ok: FalseFlag,
            code,
            message: message.into(),
            detail: None,
        })
    }
}

impl<T> From<CoreResult<T>> for CommandResult<T> {
    fn from(result: CoreResult<T>) -> Self {
        match result {
            Ok(data) => CommandResult::ok(data),
            Err(error) => CommandResult::from(error),
        }
    }
}

impl<T> From<CoreError> for CommandResult<T> {
    fn from(error: CoreError) -> Self {
        CommandResult::Failure(CommandFailure {
            ok: FalseFlag,
            code: error.code(),
            message: error.to_string(),
            detail: None,
        })
    }
}

// 让 `TrueFlag` / `FalseFlag` 在结构体里也满足调试输出需求。
impl std::fmt::Debug for TrueFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("true")
    }
}

impl std::fmt::Debug for FalseFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("false")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;

    /// 内核会产出的错误码全集。新增变体时必须同步这里与前端 `protocol.ts`。
    const EXPECTED_CODES: [&str; 9] = [
        "E_DB",
        "E_MIGRATION",
        "E_INVALID_INPUT",
        "E_PACK_INVALID",
        "E_NOT_FOUND",
        "E_IO",
        "E_NETWORK_OFF",
        "E_MODEL_UNAVAILABLE",
        "E_MALFORMED_RESPONSE",
    ];

    /// 逐个构造 `CoreError` 的每个变体，保证错误码映射被完整覆盖。
    fn every_error() -> Vec<CoreError> {
        let sqlite = || rusqlite::Error::QueryReturnedNoRows;
        vec![
            CoreError::Db(sqlite()),
            CoreError::Migration {
                version: 14,
                name: "0014_governance".to_string(),
                source: sqlite(),
            },
            CoreError::InvalidInput("参数不合法".to_string()),
            CoreError::PackInvalid(vec!["缺少来源标注".to_string()]),
            CoreError::NotFound("大师包".to_string()),
            CoreError::Io(std::io::Error::other("磁盘不可写")),
            CoreError::NetworkOff("联网能力未开启".to_string()),
            CoreError::ModelUnavailable {
                status: 503,
                message: "平台不可用".to_string(),
            },
            CoreError::MalformedResponse("缺少 choices".to_string()),
        ]
    }

    /// 解析前端 `COMMAND_ERROR_CODES`。
    ///
    /// 用 `include_str!` 引用真实的 TypeScript 源文件：文件被移动会直接变成编译
    /// 错误，清单被改动会让下面的一致性用例失败。
    fn frontend_error_codes() -> BTreeSet<String> {
        const PROTOCOL_TS: &str = include_str!("../../src/ipc/protocol.ts");
        let start = PROTOCOL_TS
            .find("COMMAND_ERROR_CODES")
            .expect("protocol.ts 应定义 COMMAND_ERROR_CODES");
        let block = &PROTOCOL_TS[start..];
        let end = block
            .find(']')
            .expect("COMMAND_ERROR_CODES 应是数组字面量");
        // 引号成对出现，取每一段的奇数下标即为字面量内容。
        block[..end]
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect()
    }

    #[test]
    fn success_shape_is_ok_and_data() {
        let value = serde_json::to_value(CommandResult::ok(json!({ "count": 2 }))).unwrap();
        assert_eq!(value, json!({ "ok": true, "data": { "count": 2 } }));
    }

    #[test]
    fn failure_shape_omits_absent_detail() {
        let value =
            serde_json::to_value(CommandResult::<()>::failure("E_INVALID_INPUT", "缺少参数"))
                .unwrap();
        assert_eq!(
            value,
            json!({ "ok": false, "code": "E_INVALID_INPUT", "message": "缺少参数" })
        );
        assert!(value.get("detail").is_none(), "无 detail 时不应出现该字段");
    }

    #[test]
    fn core_error_becomes_failure_with_stable_code() {
        for error in every_error() {
            let expected = error.code();
            let value = serde_json::to_value(CommandResult::<()>::from(error)).unwrap();
            assert_eq!(value["ok"], json!(false));
            assert_eq!(value["code"], json!(expected));
            assert!(
                value["message"].as_str().is_some_and(|text| !text.is_empty()),
                "失败结果必须带可读信息"
            );
        }
    }

    #[test]
    fn core_result_converts_on_both_arms() {
        let ok: CommandResult<i64> = CommandResult::from(Ok::<i64, CoreError>(7));
        assert_eq!(serde_json::to_value(ok).unwrap(), json!({ "ok": true, "data": 7 }));

        let failed: CommandResult<i64> = CommandResult::from(Err::<i64, _>(CoreError::NotFound(
            "会诊".to_string(),
        )));
        let value = serde_json::to_value(failed).unwrap();
        assert_eq!(value["ok"], json!(false));
        assert_eq!(value["code"], json!("E_NOT_FOUND"));
    }

    #[test]
    fn core_error_codes_and_frontend_list_stay_in_sync() {
        let core: BTreeSet<String> = every_error()
            .iter()
            .map(|error| error.code().to_string())
            .collect();
        let expected: BTreeSet<String> = EXPECTED_CODES.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            core, expected,
            "内核错误码清单发生变化，请同步 every_error、EXPECTED_CODES 与前端 protocol.ts"
        );

        let frontend = frontend_error_codes();
        assert!(!frontend.is_empty(), "解析前端错误码清单失败");

        for code in &core {
            assert!(
                frontend.contains(code),
                "前端 COMMAND_ERROR_CODES 缺少内核错误码 {code}"
            );
        }
        // 前端清单是内核错误码的镜像，只允许额外的客户端兜底码。
        for code in &frontend {
            assert!(
                core.contains(code) || code == "E_UNKNOWN",
                "前端存在内核不会产出的错误码 {code}"
            );
        }
    }
}
