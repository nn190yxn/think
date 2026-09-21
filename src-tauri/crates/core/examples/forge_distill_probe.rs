//! 真机蒸馏探针：在「现库快照」上把六阶段流水线跑完，验 V7（有已完成任务、
//! 大师版本升到 2、版本记录单元数与实际一致）。
//!
//! 正式库只读：快照用 `VACUUM INTO` 生成到临时目录，蒸馏全程在快照上做。
//! 模型调用用探针内的假客户端回结构化产出——HTTP 客户端那条路已由会诊真机跑验过，
//! 这里验的是流水线本身：阶段推进、检查点、技能单元、版本安装。
//!
//! 用法：
//!   cargo run -p thought-forge-core --example forge_distill_probe -- <forge.db 路径>
//! 跑完会打印快照库路径，接着用真机检查器判定 V7：
//!   cargo run -p thought-forge-core --example forge_verify -- <快照库路径> --expect-distill

use std::path::PathBuf;

use rusqlite::Connection;

use thought_forge_core::backup;
use thought_forge_core::db;
use thought_forge_core::distill::{pipeline, IntakeMaterial};
use thought_forge_core::llm::{GatedClient, ModelClient, ModelRequest, ModelResponse, RetryPolicy};

/// 假模型客户端：按提示词里的阶段特征回结构化产出，字段与核心侧解析结构对齐。
struct FakeClient;

impl ModelClient for FakeClient {
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, thought_forge_core::CoreError> {
        // 阶段看系统提示词判定（用户提示词里带材料与骨架草稿，容易误判）；
        // 要回填候选 id 时从用户提示词里取，候选清单只在那里。
        Ok(ModelResponse {
            content: stage_reply(&request.system, &request.user),
            platform: "probe-fake".to_string(),
            model: "probe-1".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
        })
    }
}

/// 抓提示词里 `[候选id]` 形式的标识，用来在验证与合成阶段回同样的 id。
fn ids_of(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        let inner = after[..end].trim();
        if !inner.is_empty() && inner.len() <= 64 {
            found.push(inner.to_string());
        }
        rest = &after[end + 1..];
    }
    found
}

fn stage_reply(system: &str, user: &str) -> String {
    let text = system;
    if text.contains("三重验证") {
        let items: Vec<String> = ids_of(user)
            .into_iter()
            .map(|id| {
                format!(
                    "{{\"id\":\"{id}\",\"crossDomain\":true,\"answersNew\":true,\"reason\":\"探针判定：有独立佐证\"}}"
                )
            })
            .collect();
        return format!("[{}]", items.join(","));
    }
    if text.contains("技能单元") {
        let items: Vec<String> = ids_of(user)
            .into_iter()
            .enumerate()
            .map(|(index, id)| {
                format!(
                    "{{\"candidateId\":\"{id}\",\"title\":\"探针技能单元{}\",\"layer\":\"方法层\",\
                     \"triggerCondition\":\"需要判断边界与代价时\",\"steps\":[\"先定不可退让的部分\",\"再算可承受的代价\"],\
                     \"mechanism\":\"把判断拆成边界与代价两问\",\"boundary\":\"只适用于材料覆盖到的情形\",\
                     \"evidence\":[\"探针材料\"]}}",
                    index + 1
                )
            })
            .collect();
        return format!("[{}]", items.join(","));
    }
    if text.contains("压力测试") {
        return "[{\"question\":\"探针压力题：这条判断的边界在哪里？\",\"decoy\":false,\
                 \"expected\":\"说清适用边界\",\"answer\":\"先说边界，再说代价\",\"passed\":true}]"
            .to_string();
    }
    if text.contains("整体理解") {
        return "{\"summary\":\"探针骨架：先看动机与边界，再看时机与代价。\",\"domain\":\"通用\",\
                 \"layers\":[\"原则层\",\"方法层\",\"案例层\"],\"themes\":[\"边界\",\"时机\",\"代价\"],\
                 \"angles\":[\"反例\",\"成本\"]}"
            .to_string();
    }
    format!(
        "[{{\"title\":\"探针候选{}：先定边界再算代价\",\"summary\":\"先定不可退让的部分，再算可承受的代价。\",\
          \"layer\":\"方法层\",\"evidence\":[\"探针材料\"]}}]",
        next_candidate_no()
    )
}

/// 五路提取每路的候选标题都要不一样，否则会被当成重复候选丢掉。
fn next_candidate_no() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed) + 1
}

/// 失败时把任务行里非空的字段打出来，省得肉眼猜。
fn dump_job(conn: &Connection, job_id: &str) {
    let mut stmt = match conn.prepare("SELECT * FROM distill_jobs WHERE id = ?1") {
        Ok(stmt) => stmt,
        Err(error) => {
            println!("  任务行读不出来：{error}");
            return;
        }
    };
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    let mut rows = match stmt.query([job_id]) {
        Ok(rows) => rows,
        Err(error) => {
            println!("  任务行查不出来：{error}");
            return;
        }
    };
    if let Ok(Some(row)) = rows.next() {
        for (index, name) in names.iter().enumerate() {
            let value = match row.get_ref(index) {
                Ok(value) => format!("{value:?}"),
                Err(error) => format!("<{error}>"),
            };
            if !value.contains("Null") {
                println!("  {name} = {value}");
            }
        }
    }
}

fn main() {
    let Some(db_path) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("用法：forge_distill_probe <forge.db 路径>");
        std::process::exit(2);
    };
    if let Err(error) = run(&db_path) {
        eprintln!("蒸馏探针失败：{error}");
        std::process::exit(1);
    }
}

fn run(db_path: &std::path::Path) -> Result<(), String> {
    let work = std::env::temp_dir().join("thought-forge-distill-probe");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|error| error.to_string())?;

    let (live, version) = db::initialize(db_path).map_err(|error| error.to_string())?;
    println!("现库：{}（版本 {version}）", db_path.display());
    let snapshot = backup::create_file(&live, &work, backup::KIND_MANUAL).map_err(|e| e.to_string())?;
    let snapshot_path = PathBuf::from(&snapshot.path);
    println!("快照库：{}", snapshot_path.display());

    let mut conn = Connection::open(&snapshot_path).map_err(|error| error.to_string())?;

    let (master_id, master_name, domain): (String, String, String) = conn
        .query_row(
            "SELECT id, name, COALESCE(domain, '通用') FROM masters ORDER BY rowid LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| format!("取一位大师失败：{error}"))?;
    println!("蒸馏对象：{master_name}（{master_id}）");

    let material = IntakeMaterial {
        title: "探针材料：边界与代价".to_string(),
        kind: "note".to_string(),
        source_ref: String::new(),
        text: "做判断先看动机与边界，再看时机与代价。不可退让的部分先定下来，\
               可承受的代价算清楚，最后留出复盘的时点。"
            .to_string(),
    };

    let input = pipeline::DistillInput {
        master_id,
        master_name,
        domain,
        source_kind: "note".to_string(),
        source_ref: "probe".to_string(),
        materials: vec![material],
        output_dir: work.join("packs"),
        negative: Vec::new(),
    };

    let inner = FakeClient;
    let client = GatedClient {
        enabled: true,
        inner: &inner,
    };
    let policy = RetryPolicy {
        attempts: 3,
        base_delay_ms: 0,
    };

    let mut job = pipeline::start(&mut conn, &client, &policy, &input).map_err(|e| e.to_string())?;
    println!(
        "阶段0 完成：阶段={} 状态={:?}",
        job.stage.as_str(),
        job.state
    );

    for _ in 0..12 {
        let state = format!("{:?}", job.state);
        if state.contains("Completed") || state.contains("Done") {
            break;
        }
        if state.contains("Failed") {
            dump_job(&conn, &job.id);
            return Err(format!(
                "流水线失败：阶段={} 状态={:?}",
                job.stage.as_str(),
                job.state
            ));
        }
        job = if state.contains("AwaitingConfirmation") {
            println!("骨架确认门：按应用同一条路径确认后继续");
            pipeline::confirm_skeleton(&mut conn, &client, &policy, &job.id)
                .map_err(|e| e.to_string())?
        } else {
            pipeline::advance(&mut conn, &client, &policy, &job.id).map_err(|e| e.to_string())?
        };
        println!("  推进：阶段={} 状态={:?}", job.stage.as_str(), job.state);
    }

    let units: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM master_units WHERE master_id = ?1",
            [&job.master_id],
            |row| row.get(0),
        )
        .unwrap_or(-1);
    let versions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM master_versions WHERE master_id = ?1",
            [&job.master_id],
            |row| row.get(0),
        )
        .unwrap_or(-1);
    println!(
        "结果：任务状态={:?} 阶段={} 技能单元 {units} 条 版本记录 {versions} 条",
        job.state,
        job.stage.as_str()
    );
    println!("下一步：cargo run -p thought-forge-core --example forge_verify -- {} --expect-distill", snapshot_path.display());
    Ok(())
}
