//! 真机凭据探针（默认不跑）：验系统凭据库的写入、读回与「重启后仍能解到密钥」。
//!
//! 用一条专用平台代号 `probe-credential`，值是一串测试串；它只写进系统凭据库，
//! 库里只留引用名。想清理就在界面的凭据引用里删掉这一条。
//!
//! 用法：
//!   set THOUGHT_FORGE_DB=...\forge.db
//!   set THOUGHT_FORGE_CRED_ACTION=store   # 第一个进程：写入并读回
//!   cargo test -p thought-forge-desktop --lib credential_probe_on_real_machine -- --ignored --nocapture
//!   set THOUGHT_FORGE_CRED_ACTION=load    # 第二个进程：只读回，验证跨进程仍在
//!   cargo test -p thought-forge-desktop --lib credential_probe_on_real_machine -- --ignored --nocapture

#![cfg(test)]

use std::path::Path;

use thought_forge_core::credential::{self, CredentialStore, SCOPE_PLATFORM};

use crate::credential::{resolve_api_key, ShellCredentialStore};

const PLATFORM_CODE: &str = "probe-credential";
const SECRET: &str = "probe-secret-2f8a41c6";

#[test]
#[ignore = "真机探针：会往系统凭据库写一条测试凭据"]
fn credential_probe_on_real_machine() {
    let db_path = std::env::var("THOUGHT_FORGE_DB").expect("需要 THOUGHT_FORGE_DB");
    let action = std::env::var("THOUGHT_FORGE_CRED_ACTION").unwrap_or_else(|_| "store".to_string());
    let (conn, _) = thought_forge_core::db::initialize(Path::new(&db_path)).expect("库可打开");
    let store = ShellCredentialStore::new();
    let ref_name = credential::ref_name_for(SCOPE_PLATFORM, PLATFORM_CODE).expect("引用名可生成");
    println!("系统凭据条目：{ref_name}");

    if action == "store" {
        credential::set(&conn, &store, SCOPE_PLATFORM, PLATFORM_CODE, SECRET).expect("可写入凭据库");
        println!("已写入系统凭据库（值不在库里，只在系统凭据库）");
    }

    let from_store = store.get(&ref_name).expect("可读凭据库");
    match &from_store {
        Some(value) if value == SECRET => println!("[凭据库直读] 解到密钥，长度 {} 位", value.len()),
        Some(value) => panic!("解到的值与写入的不一致，长度 {} 位", value.len()),
        None => panic!("凭据库里没有这条密钥"),
    }

    let resolved = resolve_api_key(PLATFORM_CODE);
    assert_eq!(resolved, SECRET, "应用解析出的密钥应与写入的一致");
    println!("[应用解析] 与写入值一致");

    let refs = credential::refs(&conn, SCOPE_PLATFORM).expect("可列引用");
    let ours = refs.iter().filter(|item| item.ref_name == ref_name).count();
    println!("[库内引用] 平台范围共 {} 条，其中本条 {ours} 条", refs.len());
    assert_eq!(ours, 1, "库里应恰好登记一条引用");

    println!("[下一步] 用 forge_verify --secret {SECRET} 复查 V4/V15：密钥本体不在库里");
}
