//! 真机连接器探针（默认不跑）。
//!
//! 在真库上把三类连接器（检索 / 网页 / MCP）各配一个、各真调一次，并按应用里
//! 「测试连接」的同一套写法把结果写进调用审计。用它可以把真机验收的 V13 在
//! 没有外网的情况下验完。
//!
//! 用法（先起替身服务：`node tools/stub-model-server.js 8899`）：
//!   set THOUGHT_FORGE_DB=C:\Users\<你>\AppData\Roaming\com.thoughtforge.desktop\forge.db
//!   set THOUGHT_FORGE_STUB=http://127.0.0.1:8899
//!   cargo test -p thought-forge-desktop --lib connector_probe_on_real_db -- --ignored --nocapture
//!
//! 与界面路径的差别：界面上的「测试连接」会先走一次预演确认（`guard::prepare_query`
//! 与指纹核对）再发请求；探针直接置为 ready 并发请求，预演与应用侧的脱敏另有用例和
//! 真机检查（V14）覆盖。

#![cfg(test)]

use std::path::Path;
use std::time::Instant;

use thought_forge_core::connector::{
    repo as connector_repo, validate_tools, ConnectorInput, PageReader, SearchProvider, KIND_MCP,
    KIND_PAGE, KIND_SEARCH, PURPOSE_TEST,
};
use thought_forge_core::db;

use crate::connector::{page_reader, search_provider, timeout_from_db, tool_provider};

#[test]
#[ignore = "真机探针：需要 THOUGHT_FORGE_DB 与本地替身服务"]
fn connector_probe_on_real_db() {
    let db_path = std::env::var("THOUGHT_FORGE_DB").expect("需要 THOUGHT_FORGE_DB");
    let base = std::env::var("THOUGHT_FORGE_STUB").unwrap_or_else(|_| "http://127.0.0.1:8899".to_string());
    let base = base.trim_end_matches('/').to_string();

    let (conn, version) = db::initialize(Path::new(&db_path)).expect("库可打开");
    println!("库：{db_path}（版本 {version}）");
    println!("替身服务：{base}");
    let timeout = timeout_from_db(&conn).expect("超时可读");

    let cases = [
        (KIND_SEARCH, base.clone(), "连接测试 检索", 2usize),
        (KIND_PAGE, format!("{base}/article"), "连接测试 网页", 1usize),
        (KIND_MCP, format!("{base}/mcp"), "连接测试 工具", 1usize),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (kind, endpoint, query, limit) in cases {
        let input = ConnectorInput {
            id: None,
            kind: kind.to_string(),
            display_name: format!("替身 {kind}"),
            endpoint: endpoint.clone(),
            config: serde_json::json!({}),
        };
        let view = connector_repo::upsert(&conn, &input).expect("连接器可保存");
        connector_repo::set_enabled(&conn, &view.id, true).expect("连接器可启用");
        // 省去界面上的预演确认：探针把状态直接置为 ready，好让装配层认它。
        conn.execute("UPDATE connectors SET status = 'ready' WHERE id = ?1", [&view.id])
            .expect("状态可写");

        let started = Instant::now();
        let outcome = match kind {
            KIND_SEARCH => search_provider(&endpoint, Some(&view.id), timeout).and_then(|provider| {
                provider
                    .search(query, limit)
                    .map(|hits| format!("检索可用，返回 {} 条结果", hits.len()))
            }),
            KIND_PAGE => page_reader(timeout).and_then(|provider| {
                provider
                    .read(&endpoint)
                    .map(|page| format!("网页可读，标题「{}」正文 {} 字", page.title, page.text.chars().count()))
            }),
            _ => tool_provider(&endpoint, Some(&view.id)).and_then(|provider| {
                validate_tools(&provider).map(|tools| format!("工具服务器可用，声明 {} 个工具", tools.len()))
            }),
        };
        let latency = started.elapsed().as_millis() as i64;
        let (status, error_code, note) = match &outcome {
            Ok(text) => ("ok".to_string(), None, text.clone()),
            Err(error) => {
                let note = error.to_string();
                failures.push(format!("{kind}：{note}"));
                ("failed".to_string(), Some(error.code().to_string()), note)
            }
        };

        // 与 `commands::connector_test` 同表同字段。
        connector_repo::record_call(
            &conn,
            &connector_repo::ConnectorCallRecord {
                connector_id: Some(view.id.clone()),
                kind: kind.to_string(),
                purpose: PURPOSE_TEST.to_string(),
                session_id: None,
                query: query.to_string(),
                query_sent: query.to_string(),
                redacted: false,
                result_count: if status == "ok" { 1 } else { 0 },
                cost_micros: 0,
                latency_ms: latency,
                status: status.clone(),
                error_code: error_code.clone(),
            },
        )
        .expect("调用审计可写入");

        println!("  {kind}：{status} · {note} · {latency}ms（连接器 {}）", view.id);
    }

    let listed = connector_repo::list(&conn).expect("连接器可列出");
    println!("库里连接器 {} 个", listed.len());

    // 收尾：默认把替身连接器关掉，免得应用里留着指向本机替身服务的死配置。
    // 连接器行与调用审计都保留，V13 看的是「配过、调过」。
    if std::env::var("THOUGHT_FORGE_CONNECTOR_OFF").is_ok() {
        let mut closed = 0;
        for view in &listed {
            if view.display_name.starts_with("替身 ") && view.enabled {
                connector_repo::set_enabled(&conn, &view.id, false).expect("连接器可关闭");
                closed += 1;
            }
        }
        println!("已关闭 {closed} 个替身连接器（配置与调用审计保留）");
    }

    assert!(failures.is_empty(), "有连接器调用失败：{}", failures.join(" / "));
}
