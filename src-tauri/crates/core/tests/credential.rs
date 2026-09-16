//! 凭据引用：引用名生成、密钥不入库、写入失败路径与状态查询。

use std::cell::RefCell;
use std::collections::HashMap;

use rusqlite::Connection;
use thought_forge_core::credential::{self, CredentialStore, SCOPE_CONNECTOR, SCOPE_PLATFORM};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::CoreError;

/// 内存凭据库替身，可选地让写入失败以验证失败路径。
#[derive(Default)]
struct MemoryStore {
    secrets: RefCell<HashMap<String, String>>,
    fail_put: bool,
}

impl MemoryStore {
    fn failing() -> Self {
        Self {
            secrets: RefCell::new(HashMap::new()),
            fail_put: true,
        }
    }

    fn contains(&self, ref_name: &str) -> bool {
        self.secrets.borrow().contains_key(ref_name)
    }
}

impl CredentialStore for MemoryStore {
    fn put(&self, ref_name: &str, secret: &str) -> thought_forge_core::CoreResult<()> {
        if self.fail_put {
            return Err(CoreError::Io(std::io::Error::other("凭据库不可用")));
        }
        self.secrets
            .borrow_mut()
            .insert(ref_name.to_string(), secret.to_string());
        Ok(())
    }

    fn get(&self, ref_name: &str) -> thought_forge_core::CoreResult<Option<String>> {
        Ok(self.secrets.borrow().get(ref_name).cloned())
    }

    fn delete(&self, ref_name: &str) -> thought_forge_core::CoreResult<()> {
        self.secrets.borrow_mut().remove(ref_name);
        Ok(())
    }
}

fn db() -> Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

#[test]
fn ref_name_validates_scope_and_owner() {
    assert_eq!(
        credential::ref_name_for(SCOPE_PLATFORM, "cloud").unwrap(),
        "thought-forge/platform/cloud"
    );
    assert_eq!(
        credential::ref_name_for(SCOPE_CONNECTOR, "search-1").unwrap(),
        "thought-forge/connector/search-1"
    );
    assert!(credential::ref_name_for("unknown", "cloud").is_err());
    assert!(credential::ref_name_for(SCOPE_PLATFORM, "  ").is_err());
}

#[test]
fn set_registers_reference_without_storing_secret() {
    let conn = db();
    let store = MemoryStore::default();
    let view = credential::set(&conn, &store, SCOPE_PLATFORM, "cloud", "sk-secret-value").unwrap();

    assert_eq!(view.ref_name, "thought-forge/platform/cloud");
    assert_eq!(view.scope, SCOPE_PLATFORM);
    assert_eq!(view.owner_id, "cloud");
    assert!(store.contains(&view.ref_name), "密钥应写入凭据库");

    // 数据库只保存引用名，任何字段都不应包含密钥正文。
    let refs = credential::refs(&conn, "").unwrap();
    assert_eq!(refs.len(), 1);
    let dumped = format!("{refs:?}");
    assert!(!dumped.contains("sk-secret-value"), "数据库中不得出现密钥");
}

#[test]
fn status_reflects_store_presence() {
    let conn = db();
    let store = MemoryStore::default();
    let ref_name = "thought-forge/connector/search-1";
    assert!(!credential::status(&conn, &store, ref_name).unwrap());

    credential::set(&conn, &store, SCOPE_CONNECTOR, "search-1", "token").unwrap();
    assert!(credential::status(&conn, &store, ref_name).unwrap());

    store.delete(ref_name).unwrap();
    assert!(!credential::status(&conn, &store, ref_name).unwrap());
}

#[test]
fn repeated_set_updates_reference_without_duplicates() {
    let conn = db();
    let store = MemoryStore::default();
    credential::set(&conn, &store, SCOPE_PLATFORM, "cloud", "first").unwrap();
    let second = credential::set(&conn, &store, SCOPE_PLATFORM, "cloud", "second").unwrap();

    assert_eq!(credential::refs(&conn, SCOPE_PLATFORM).unwrap().len(), 1);
    assert_eq!(store.get(&second.ref_name).unwrap().as_deref(), Some("second"));
}

#[test]
fn failed_store_write_leaves_no_reference() {
    let conn = db();
    let store = MemoryStore::failing();
    let error = credential::set(&conn, &store, SCOPE_PLATFORM, "cloud", "secret").unwrap_err();
    assert_eq!(error.code(), "E_IO");
    assert!(
        credential::refs(&conn, SCOPE_PLATFORM).unwrap().is_empty(),
        "凭据库写入失败时不应登记引用"
    );
}
