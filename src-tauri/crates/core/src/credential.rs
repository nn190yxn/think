//! 凭据引用：数据库只保存引用名，密钥本体交操作系统凭据库保管。
//!
//! 与既有「密钥不入库」约束一致：`credential_refs` 表里没有密钥字段，
//! 真实值由外壳通过 [`CredentialStore`] 实现写入系统凭据库。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

pub const SCOPE_PLATFORM: &str = "platform";
pub const SCOPE_CONNECTOR: &str = "connector";
/// 引用名前缀，避免与其它应用在系统凭据库中撞名。
pub const REF_PREFIX: &str = "thought-forge";

/// 操作系统凭据库抽象。外壳提供真实实现，测试用内存替身。
pub trait CredentialStore {
    fn put(&self, ref_name: &str, secret: &str) -> CoreResult<()>;
    fn get(&self, ref_name: &str) -> CoreResult<Option<String>>;
    fn delete(&self, ref_name: &str) -> CoreResult<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRefView {
    pub id: String,
    pub ref_name: String,
    pub scope: String,
    pub owner_id: String,
    pub created_at: String,
    pub updated_at: String,
}

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

/// 校验凭据范围并生成引用名。
pub fn ref_name_for(scope: &str, owner_id: &str) -> CoreResult<String> {
    let scope = scope.trim();
    if scope != SCOPE_PLATFORM && scope != SCOPE_CONNECTOR {
        return Err(CoreError::InvalidInput(format!("未知的凭据范围：{scope}")));
    }
    let owner_id = owner_id.trim();
    if owner_id.is_empty() {
        return Err(CoreError::InvalidInput("凭据归属标识不能为空".to_string()));
    }
    Ok(format!("{REF_PREFIX}/{scope}/{owner_id}"))
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialRefView> {
    Ok(CredentialRefView {
        id: row.get(0)?,
        ref_name: row.get(1)?,
        scope: row.get(2)?,
        owner_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

/// 登记一个凭据引用。同一引用名重复登记只更新归属与时间。
pub fn register(
    conn: &Connection,
    scope: &str,
    owner_id: &str,
    ref_name: &str,
) -> CoreResult<CredentialRefView> {
    let expected = ref_name_for(scope, owner_id)?;
    if ref_name.trim() != expected {
        return Err(CoreError::InvalidInput(format!(
            "引用名与范围不匹配，应为 {expected}"
        )));
    }
    let now_value = now(conn)?;
    let id = unique_id("cred", &expected);
    conn.execute(
        "INSERT INTO credential_refs (id, ref_name, scope, owner_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(ref_name) DO UPDATE SET
             scope = excluded.scope,
             owner_id = excluded.owner_id,
             updated_at = excluded.updated_at",
        rusqlite::params![id, expected, scope.trim(), owner_id.trim(), now_value],
    )?;
    view(conn, &expected)
}

/// 写入密钥并登记引用。先写凭据库，再登记引用名。
pub fn set(
    conn: &Connection,
    store: &dyn CredentialStore,
    scope: &str,
    owner_id: &str,
    secret: &str,
) -> CoreResult<CredentialRefView> {
    if secret.is_empty() {
        return Err(CoreError::InvalidInput("密钥不能为空".to_string()));
    }
    let ref_name = ref_name_for(scope, owner_id)?;
    store.put(&ref_name, secret)?;
    register(conn, scope, owner_id, &ref_name)
}

/// 读取一个引用。
pub fn view(conn: &Connection, ref_name: &str) -> CoreResult<CredentialRefView> {
    conn.query_row(
        "SELECT id, ref_name, scope, owner_id, created_at, updated_at
         FROM credential_refs WHERE ref_name = ?1",
        [ref_name],
        from_row,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("凭据引用 {ref_name}")))
}

/// 按范围列出引用。范围为空串时返回全部。
pub fn refs(conn: &Connection, scope: &str) -> CoreResult<Vec<CredentialRefView>> {
    let scope = scope.trim();
    let mut stmt = conn.prepare(
        "SELECT id, ref_name, scope, owner_id, created_at, updated_at
         FROM credential_refs
         WHERE (?1 = '' OR scope = ?1)
         ORDER BY created_at ASC, rowid ASC",
    )?;
    let rows = stmt.query_map([scope], from_row)?;
    let mut list = Vec::new();
    for row in rows {
        list.push(row?);
    }
    Ok(list)
}

/// 凭据库里是否已存在该引用的密钥。引用未登记时同样返回假。
pub fn status(conn: &Connection, store: &dyn CredentialStore, ref_name: &str) -> CoreResult<bool> {
    let registered = conn
        .query_row(
            "SELECT COUNT(*) FROM credential_refs WHERE ref_name = ?1",
            [ref_name],
            |row| row.get::<_, i64>(0),
        )?
        > 0;
    if !registered {
        return Ok(false);
    }
    Ok(store.get(ref_name)?.is_some())
}
