//! 桌面外壳的凭据库实现：密钥写入操作系统凭据库，数据库只保留引用名。
//!
//! 环境变量保留为回退路径：系统凭据库不可用或没有对应条目时，
//! 模型装配退回读取 [`API_KEY_ENV`]，保证无桌面密钥环的场景也能用。

use keyring::Entry;

use thought_forge_core::credential::{
    ref_name_for, CredentialStore, REF_PREFIX, SCOPE_PLATFORM,
};
use thought_forge_core::{CoreError, CoreResult};

use crate::model::API_KEY_ENV;

/// 系统凭据库中使用的条目服务名。
pub const SERVICE: &str = REF_PREFIX;

/// 基于 `keyring` 的系统凭据库读写。
pub struct ShellCredentialStore;

impl ShellCredentialStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShellCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

fn io_err(message: String) -> CoreError {
    CoreError::Io(std::io::Error::other(message))
}

fn entry(ref_name: &str) -> CoreResult<Entry> {
    Entry::new(SERVICE, ref_name)
        .map_err(|error| io_err(format!("系统凭据库不可用：{error}")))
}

impl CredentialStore for ShellCredentialStore {
    fn put(&self, ref_name: &str, secret: &str) -> CoreResult<()> {
        entry(ref_name)?
            .set_password(secret)
            .map_err(|error| io_err(format!("写入凭据库失败：{error}")))
    }

    fn get(&self, ref_name: &str) -> CoreResult<Option<String>> {
        match entry(ref_name)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(io_err(format!("读取凭据库失败：{error}"))),
        }
    }

    fn delete(&self, ref_name: &str) -> CoreResult<()> {
        match entry(ref_name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(io_err(format!("删除凭据失败：{error}"))),
        }
    }
}

/// 解析某个平台要用的密钥：先查系统凭据库，再退回环境变量。
pub fn resolve_api_key(platform_code: &str) -> String {
    // 引用名统一走内核的命名规则，避免与登记引用时的写法漂移。
    if let Ok(ref_name) = ref_name_for(SCOPE_PLATFORM, platform_code) {
        if let Ok(Some(secret)) = ShellCredentialStore::new().get(&ref_name) {
            if !secret.is_empty() {
                return secret;
            }
        }
    }
    std::env::var(API_KEY_ENV).unwrap_or_default()
}

/// 是否已为某平台解析到非空密钥。
///
/// 只回答存在与否，绝不返回密钥内容，供自检探针报告 `keyPresent`。
pub fn has_api_key(platform_code: &str) -> bool {
    !resolve_api_key(platform_code).trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_ref_name_matches_registered_rule() {
        // 与内核 `ref_name_for` 及前端 `thought-forge/${scope}/${ownerId}` 保持同一写法。
        assert_eq!(
            ref_name_for(SCOPE_PLATFORM, "cloud").unwrap(),
            "thought-forge/platform/cloud"
        );
    }

    #[test]
    fn empty_owner_has_no_ref_name() {
        // 归属为空时没有合法引用名，解析密钥会直接退回环境变量。
        assert!(ref_name_for(SCOPE_PLATFORM, "   ").is_err());
    }
}
