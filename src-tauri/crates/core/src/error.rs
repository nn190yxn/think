use std::fmt;

/// 内核错误。`code` 与前端 `COMMAND_ERROR_CODES` 一一对应。
#[derive(Debug)]
pub enum CoreError {
    /// 数据库本身的错误。
    Db(rusqlite::Error),
    /// 某个迁移执行失败，带上版本号便于定位。
    Migration {
        version: i64,
        name: String,
        source: rusqlite::Error,
    },
    /// 输入不合法，不涉及数据库。
    InvalidInput(String),
    /// 大师包校验未通过，附全部未通过项。
    PackInvalid(Vec<String>),
    /// 目标记录不存在。
    NotFound(String),
    /// 文件系统错误。
    Io(std::io::Error),
    /// 联网能力未开启时发起了外部请求。
    NetworkOff(String),
    /// 模型平台不可用或返回失败。
    ModelUnavailable { status: i64, message: String },
    /// 返回数据不符合约定结构。
    MalformedResponse(String),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::Migration { .. } => "E_MIGRATION",
            CoreError::InvalidInput(_) => "E_INVALID_INPUT",
            CoreError::PackInvalid(_) => "E_PACK_INVALID",
            CoreError::NotFound(_) => "E_NOT_FOUND",
            CoreError::Io(_) => "E_IO",
            CoreError::Db(_) => "E_DB",
            CoreError::NetworkOff(_) => "E_NETWORK_OFF",
            CoreError::ModelUnavailable { .. } => "E_MODEL_UNAVAILABLE",
            CoreError::MalformedResponse(_) => "E_MALFORMED_RESPONSE",
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Db(source) => write!(f, "数据库错误：{source}"),
            CoreError::Migration {
                version,
                name,
                source,
            } => write!(f, "迁移 {version}({name}) 执行失败：{source}"),
            CoreError::InvalidInput(message) => write!(f, "输入不合法：{message}"),
            CoreError::PackInvalid(issues) => {
                write!(f, "大师包校验未通过（{} 项）：{}", issues.len(), issues.join("；"))
            }
            CoreError::NotFound(what) => write!(f, "未找到：{what}"),
            CoreError::Io(source) => write!(f, "文件读写失败：{source}"),
            CoreError::NetworkOff(what) => write!(f, "联网能力未开启：{what}"),
            CoreError::ModelUnavailable { status, message } => {
                write!(f, "模型平台不可用（{status}）：{message}")
            }
            CoreError::MalformedResponse(message) => write!(f, "返回数据不合约定：{message}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CoreError::Db(source) => Some(source),
            CoreError::Io(source) => Some(source),
            CoreError::Migration { source, .. } => Some(source),
            CoreError::InvalidInput(_)
            | CoreError::PackInvalid(_)
            | CoreError::NotFound(_)
            | CoreError::NetworkOff(_)
            | CoreError::ModelUnavailable { .. }
            | CoreError::MalformedResponse(_) => None,
        }
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(source: rusqlite::Error) -> Self {
        CoreError::Db(source)
    }
}

impl From<std::io::Error> for CoreError {
    fn from(source: std::io::Error) -> Self {
        CoreError::Io(source)
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
