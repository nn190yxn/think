//! 真机会诊：用真实模型跑一场会诊，供真机验收核对 V5 / V6 与「锁定的人坐到他积累最深的题」。
//!
//! 用法：
//!   cargo run -p thought-forge-desktop --example forge_live_council -- <forge.db 路径> [问题]
//!
//! 平台按参数写入（默认 DashScope 的 OpenAI 兼容地址 + qwen-plus）。密钥不落库：
//! 先查系统凭据库，缺失时退回环境变量 `THOUGHT_FORGE_API_KEY`（本示例不打印密钥）。

use std::path::PathBuf;

use rusqlite::Connection;

use thought_forge_core::council::{orchestrator, pool, repo, select, Strategy};
use thought_forge_core::db;
use thought_forge_core::llm::platform::{self, PlatformInput};
use thought_forge_core::llm::RetryPolicy;
use thought_forge_core::master::repo as masters;
use thought_forge_desktop_lib::model;

const PLATFORM_CODE: &str = "dashscope";
const DEFAULT_ENDPOINT: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions";
const DEFAULT_MODEL: &str = "qwen-plus";

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(db_path) = args.next() else {
        eprintln!("用法：forge_live_council <forge.db 路径> [问题]");
        std::process::exit(2);
    };
    let question = args
        .next()
        .unwrap_or_else(|| "要不要把一个稳定的工作换成独立做产品，怎么判断值不值得".to_string());

    if let Err(error) = run(PathBuf::from(db_path), &question) {
        eprintln!("真机会诊失败：{error}");
        std::process::exit(1);
    }
}

fn run(db_path: PathBuf, question: &str) -> Result<(), String> {
    let (conn, version) = db::initialize(&db_path).map_err(|error| error.to_string())?;
    println!("库：{}（版本 {version}）", db_path.display());
    println!("问题：{question}");

    // 端点必须填到 /chat/completions 那一段：请求按原样当作地址用。
    let input = PlatformInput {
        code: PLATFORM_CODE.to_string(),
        display_name: "DashScope（通义千问）".to_string(),
        endpoint: DEFAULT_ENDPOINT.to_string(),
        model_name: DEFAULT_MODEL.to_string(),
        input_price_micros_per_1k: 800,
        output_price_micros_per_1k: 2000,
        currency: "CNY".to_string(),
    };
    // upsert 只写配置，启用要单独开：默认关闭，避免悄悄联网。
    let view = platform::upsert(&conn, &input).map_err(|error| error.to_string())?;
    let view =
        platform::set_enabled(&conn, &view.code, true).map_err(|error| error.to_string())?;
    println!(
        "平台：{} · {} · {}",
        view.code, view.model_name, view.status
    );
    if view.status != "ready" {
        return Err(format!("平台未就绪：{}", view.status));
    }

    let client = model::build_client(&view).map_err(|error| error.to_string())?;

    let pool = pool::build(&conn, &pool::TopicInput { question, domains: &[] })
        .map_err(|error| error.to_string())?;
    let session = repo::create_session(&conn, question, &pool.domains, &[], Strategy::Steady)
        .map_err(|error| error.to_string())?;
    let plan = select::select_panel(&conn, &pool, &select::SelectionRequest::new(Strategy::Steady))
        .map_err(|error| error.to_string())?;
    repo::record_panel(&conn, &session, 0, &plan, &[]).map_err(|error| error.to_string())?;

    let policy = RetryPolicy {
        attempts: 2,
        base_delay_ms: 500,
    };
    orchestrator::run_council(&conn, &client, &session, &policy).map_err(|error| {
        println!("会诊失败：{error}");
        dump_recent_calls(&conn);
        error.to_string()
    })?;

    if let Some(panel) = repo::latest_panel(&conn, &session).map_err(|error| error.to_string())? {
        println!("阵容 {} 席：", panel.seats.len());
        for seat in &panel.seats {
            let name = masters::detail(&conn, &seat.master_id)
                .map(|detail| detail.name)
                .unwrap_or_else(|_| seat.master_id.clone());
            println!("  · {name} → 第 {:?} 题", seat.layer);
        }
    }
    println!("调用审计：{} 条", count_calls(&conn)?);
    Ok(())
}

fn count_calls(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM llm_calls", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

/// 排查用：把最近几条调用审计原样打出来，列名也一并带上，不靠猜字段。
fn dump_recent_calls(conn: &Connection) {
    let Ok(mut stmt) = conn.prepare("SELECT * FROM llm_calls ORDER BY rowid DESC LIMIT 3") else {
        return;
    };
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    let Ok(mut rows) = stmt.query([]) else {
        return;
    };
    while let Ok(Some(row)) = rows.next() {
        let mut parts: Vec<String> = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let text = match row.get_ref(index) {
                Ok(rusqlite::types::ValueRef::Null) => "null".to_string(),
                Ok(rusqlite::types::ValueRef::Integer(value)) => value.to_string(),
                Ok(rusqlite::types::ValueRef::Real(value)) => value.to_string(),
                Ok(rusqlite::types::ValueRef::Text(bytes)) => {
                    String::from_utf8_lossy(bytes).to_string()
                }
                Ok(rusqlite::types::ValueRef::Blob(_)) => "<blob>".to_string(),
                Err(_) => "<err>".to_string(),
            };
            parts.push(format!("{name}={text}"));
        }
        println!("  {}", parts.join(" | "));
    }
}
