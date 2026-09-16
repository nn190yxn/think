//! Windows 真机验证用的只读检查器。
//!
//! 它只读数据库，不改任何数据，也不需要 Tauri 运行时，因此在真机上可以和应用
//! 同时运行。它负责把「能由数据库判定」的项目一次性查完，剩下需要动手操作的
//! 项目由验证记录表逐项走。
//!
//! 用法：
//!   cargo run -p thought-forge-core --example forge_verify -- <forge.db 路径> [选项]
//!
//! 选项：
//!   --secret <值>              扫描全库，确认该密钥未落库（V4 / V15）
//!   --expect-search            要求存在一次成功的检索调用（V11）
//!   --expect-capture <类型,...> 要求这些采集类型已有记录（V8）
//!   --expect-flagged           要求存在被标记的外部来源（V18）
//!   --expect-pre-migration     要求存在迁移前自动备份（V17）
//!
//! 退出码：0 全部通过或跳过，1 存在未通过项，2 用法错误。
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rusqlite::{Connection, OpenFlags};
use thought_forge_core::capture::{
    KIND_CLIPBOARD_IMAGE, KIND_CLIPBOARD_TEXT, KIND_FILE, KIND_WINDOW,
};
use thought_forge_core::data::DATA_TABLES;
use thought_forge_core::db::migrations;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Pass,
    Fail,
    Skip,
}

impl Verdict {
    fn tag(self) -> &'static str {
        match self {
            Verdict::Pass => "通过",
            Verdict::Fail => "未过",
            Verdict::Skip => "跳过",
        }
    }
}

struct Check {
    id: &'static str,
    title: &'static str,
    verdict: Verdict,
    lines: Vec<String>,
}

impl Check {
    fn new(id: &'static str, title: &'static str) -> Self {
        Self {
            id,
            title,
            verdict: Verdict::Pass,
            lines: Vec::new(),
        }
    }

    fn note(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    fn fail(&mut self, line: impl Into<String>) {
        self.verdict = Verdict::Fail;
        self.note(format!("未过原因：{}", line.into()));
    }

    fn skip(&mut self, line: impl Into<String>) {
        if self.verdict != Verdict::Fail {
            self.verdict = Verdict::Skip;
        }
        self.note(format!("跳过原因：{}", line.into()));
    }
}

struct Options {
    db: PathBuf,
    secret: Option<String>,
    expect_search: bool,
    expect_capture: Vec<String>,
    expect_flagged: bool,
    expect_pre_migration: bool,
}

fn usage() -> ! {
    eprintln!(
        "用法：forge_verify <forge.db 路径> [--secret <值>] [--expect-search] \
         [--expect-capture <类型,...>] [--expect-flagged] [--expect-pre-migration]"
    );
    std::process::exit(2);
}

fn parse_args() -> Options {
    let mut args = std::env::args().skip(1);
    let Some(db) = args.next() else {
        usage();
    };
    let mut options = Options {
        db: PathBuf::from(db),
        secret: None,
        expect_search: false,
        expect_capture: Vec::new(),
        expect_flagged: false,
        expect_pre_migration: false,
    };
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--secret" => {
                options.secret = args.next();
                if options.secret.is_none() {
                    usage();
                }
            }
            "--expect-search" => options.expect_search = true,
            "--expect-flagged" => options.expect_flagged = true,
            "--expect-pre-migration" => options.expect_pre_migration = true,
            "--expect-capture" => {
                let Some(value) = args.next() else {
                    usage();
                };
                options.expect_capture = value
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect();
            }
            other => {
                eprintln!("未知选项：{other}");
                usage();
            }
        }
    }
    options
}

/// 只读打开。应用可能正在运行，WAL 下并发只读是安全的。
fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
}

fn scalar_i64(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get::<_, i64>(0))
        .unwrap_or(0)
}

fn scalar_text(conn: &Connection, sql: &str) -> Option<String> {
    conn.query_row(sql, [], |row| row.get::<_, String>(0)).ok()
}

/// 迁移版本与库是否到位。
fn check_schema(conn: &Connection, checks: &mut Vec<Check>) {
    let mut check = Check::new("S1", "库可用且迁移到最新版本");
    let expected = migrations::latest_version();
    match migrations::current_version(conn) {
        Ok(version) => {
            check.note(format!("当前版本 {version}，最新版本 {expected}"));
            if version != expected {
                check.fail(format!("版本应为 {expected}，实际 {version}"));
            }
        }
        Err(error) => check.fail(error.to_string()),
    }
    checks.push(check);
}

/// V4 / V15：给定密钥不得出现在任何一张表里。
fn check_secret(conn: &Connection, options: &Options, checks: &mut Vec<Check>) {
    let mut check = Check::new("V4", "密钥不落库");
    let Some(secret) = options.secret.as_deref() else {
        check.skip("未提供 --secret，无法判定");
        checks.push(check);
        return;
    };
    if secret.trim().is_empty() {
        check.fail("--secret 为空字符串，无法判定");
        checks.push(check);
        return;
    }

    let mut hits = 0i64;
    let mut scanned = 0i64;
    for (table, label) in DATA_TABLES {
        let columns = match text_columns(conn, table) {
            Ok(columns) => columns,
            Err(_) => continue,
        };
        for column in columns {
            scanned += 1;
            // 表名与列名来自 sqlite_master，无法参数化，只能内联；用双引号包裹并转义。
            let sql = format!(
                "SELECT COUNT(*) FROM \"{}\" WHERE instr(COALESCE(\"{}\", ''), ?1) > 0",
                table.replace('"', "\"\""),
                column.replace('"', "\"\"")
            );
            let found: i64 = conn
                .query_row(&sql, [secret], |row| row.get(0))
                .unwrap_or(0);
            if found > 0 {
                hits += found;
                // 只报位置，不回显密钥本身。
                check.note(format!("命中：{label}（{table}.{column}）{found} 行"));
            }
        }
    }
    check.note(format!("扫描 {scanned} 个文本列"));
    if hits > 0 {
        check.fail(format!("密钥出现在 {hits} 处，应只保留凭据引用名"));
    }
    checks.push(check);
}

fn text_columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!(
        "PRAGMA table_info(\"{}\")",
        table.replace('"', "\"\"")
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
    })?;
    let mut columns = Vec::new();
    for row in rows {
        let (name, kind) = row?;
        // 只扫文本列；声明里含 TEXT 或没有类型声明（SQLite 允许无类型）都要看。
        if kind.is_empty() || kind.to_ascii_uppercase().contains("TEXT") {
            columns.push(name);
        }
    }
    Ok(columns)
}

/// V5：最新一条模型调用审计字段齐全。
fn check_llm_audit(conn: &Connection, checks: &mut Vec<Check>) {
    let mut check = Check::new("V5", "模型调用审计字段齐全");
    let total = scalar_i64(conn, "SELECT COUNT(*) FROM llm_calls");
    check.note(format!("llm_calls 共 {total} 行"));
    if total == 0 {
        check.skip("尚无模型调用，先跑一次 model_probe 或会诊");
        checks.push(check);
        return;
    }
    let row = conn
        .query_row(
            "SELECT purpose, platform_code, model_name, latency_ms, status, error_code, created_at
             FROM llm_calls ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .ok();
    let Some((purpose, platform, model, latency, status, error, created)) = row else {
        check.fail("无法读取最新一行");
        checks.push(check);
        return;
    };
    check.note(format!(
        "最新：用途={purpose} 平台={platform} 模型={model} 耗时={latency}ms 状态={status} \
         错误码={} 时间={created}",
        error.unwrap_or_else(|| "无".to_string())
    ));
    if purpose.trim().is_empty() || platform.trim().is_empty() || model.trim().is_empty() {
        check.fail("用途、平台或模型为空");
    }
    checks.push(check);
}

/// V8：采集记录与开关一致，关闭后不再产生新事件。
fn check_capture(conn: &Connection, options: &Options, checks: &mut Vec<Check>) {
    let mut check = Check::new("V8", "采集落库且关闭后不再产生事件");
    let kinds = [
        KIND_CLIPBOARD_TEXT,
        KIND_CLIPBOARD_IMAGE,
        KIND_WINDOW,
        KIND_FILE,
    ];

    for kind in kinds {
        let enabled = scalar_i64(
            conn,
            &format!("SELECT COALESCE((SELECT enabled FROM capture_settings WHERE kind = '{kind}'), 0)"),
        ) == 1;
        let events = scalar_i64(
            conn,
            &format!("SELECT COUNT(*) FROM capture_events WHERE kind = '{kind}'"),
        );
        let last_disable = scalar_text(
            conn,
            &format!(
                "SELECT MAX(created_at) FROM capture_audit WHERE kind = '{kind}' AND action = 'disable'"
            ),
        );
        check.note(format!(
            "{kind}：开关={} 记录={events} 最近关闭={}",
            if enabled { "开" } else { "关" },
            last_disable.as_deref().unwrap_or("无")
        ));

        // 关闭之后不应再有该类型的新事件：这是「关闭后不再产生新事件」的直接判据。
        if let Some(at) = last_disable {
            let after = scalar_i64(
                conn,
                &format!(
                    "SELECT COUNT(*) FROM capture_events
                     WHERE kind = '{kind}' AND occurred_at > '{at}'"
                ),
            );
            if after > 0 {
                check.fail(format!("{kind} 在关闭后仍新增 {after} 条记录"));
            }
        }

        if options.expect_capture.iter().any(|item| item == kind) && events == 0 {
            check.fail(format!("要求 {kind} 已有记录，但一条也没有"));
        }
    }

    // 关注目录必须可解析且逐个存在，否则文件活动只是看起来开着。
    let roots = scalar_text(
        conn,
        "SELECT value FROM settings WHERE key = 'capture.watch_roots'",
    );
    match roots.as_deref() {
        None => check.note("关注目录：未设置"),
        Some(raw) => match serde_json::from_str::<Vec<String>>(raw) {
            Ok(list) => {
                let mut bad = Vec::new();
                for item in &list {
                    let path = Path::new(item);
                    if !path.is_absolute() || !path.is_dir() {
                        bad.push(item.clone());
                    }
                }
                check.note(format!("关注目录：{} 个，{}", list.len(), list.join(" | ")));
                if !bad.is_empty() {
                    check.fail(format!("以下目录不是可用的绝对路径：{}", bad.join(" | ")));
                }
            }
            Err(error) => check.fail(format!("关注目录不是合法 JSON 数组：{error}")),
        },
    }
    checks.push(check);
}

/// V11：存在一次成功的检索调用。
fn check_search(conn: &Connection, options: &Options, checks: &mut Vec<Check>) {
    let mut check = Check::new("V11", "检索连接器连通并留有审计");
    let total = scalar_i64(conn, "SELECT COUNT(*) FROM connector_calls WHERE kind = 'search'");
    check.note(format!("检索调用 {total} 次"));
    if total == 0 {
        check.skip("尚无检索调用，先配置并启用搜索连接器");
        checks.push(check);
        return;
    }
    let ok = scalar_i64(
        conn,
        "SELECT COUNT(*) FROM connector_calls
         WHERE kind = 'search' AND status = 'ok' AND result_count > 0",
    );
    let failed = scalar_i64(
        conn,
        "SELECT COUNT(*) FROM connector_calls WHERE kind = 'search' AND status != 'ok'",
    );
    let latest = scalar_text(
        conn,
        "SELECT purpose || ' / ' || status || ' / ' || result_count || ' 条 / ' || created_at
         FROM connector_calls WHERE kind = 'search' ORDER BY created_at DESC, rowid DESC LIMIT 1",
    );
    check.note(format!(
        "成功且有结果 {ok} 次，失败 {failed} 次，最新：{}",
        latest.unwrap_or_else(|| "无".to_string())
    ));
    if options.expect_search && ok == 0 {
        check.fail("要求存在一次成功的检索，但一次都没有");
    }
    checks.push(check);
}

/// V14：实际发送串与原始问句不同，且不含原文敏感片段。
fn check_query_redaction(conn: &Connection, checks: &mut Vec<Check>) {
    let mut check = Check::new("V14", "检索发送串已脱敏");
    let total = scalar_i64(conn, "SELECT COUNT(*) FROM connector_calls WHERE kind = 'search'");
    if total == 0 {
        check.skip("尚无检索调用");
        checks.push(check);
        return;
    }

    let mut stmt = match conn.prepare(
        "SELECT COALESCE(query_original, ''), COALESCE(query_sent, ''), redacted
         FROM connector_calls WHERE kind = 'search'",
    ) {
        Ok(stmt) => stmt,
        Err(error) => {
            check.fail(error.to_string());
            checks.push(check);
            return;
        }
    };
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)? == 1,
        ))
    });
    let Ok(rows) = rows else {
        check.fail("读取检索审计失败");
        checks.push(check);
        return;
    };

    let mut untraceable = 0;
    let mut identical = 0;
    let mut leaking = Vec::new();
    for row in rows.flatten() {
        let (original, sent, redacted) = row;
        if original.trim().is_empty() {
            untraceable += 1;
        }
        if redacted && sent == original {
            identical += 1;
        }
        if let Some(hit) = sensitive_fragment(&sent) {
            leaking.push(hit);
        }
    }
    check.note(format!("已检查 {total} 次检索"));
    if untraceable > 0 {
        check.fail(format!("{untraceable} 次没有留下原始问句"));
    }
    if identical > 0 {
        check.fail(format!("{identical} 次标记为已脱敏，但发送串与原始问句完全相同"));
    }
    if !leaking.is_empty() {
        check.fail(format!(
            "发送串里仍有敏感片段：{}",
            leaking.join(" | ")
        ));
    }
    checks.push(check);
}

/// 邮箱与 11 位数字串，用来判定发送串里是否还有原文敏感片段。
fn sensitive_fragment(text: &str) -> Option<String> {
    let digits: Vec<char> = text.chars().collect();
    let mut run = 0;
    for ch in &digits {
        if ch.is_ascii_digit() {
            run += 1;
            if run >= 11 {
                return Some("疑似手机号或长数字串".to_string());
            }
        } else {
            run = 0;
        }
    }
    if let Some(at) = text.find('@') {
        let rest = &text[at + 1..];
        if rest.contains('.') && !rest.starts_with(' ') && !rest.contains(' ') {
            return Some("疑似邮箱".to_string());
        }
    }
    None
}

/// V16 / V17：备份留痕、文件存在与迁移前备份。
fn check_backups(conn: &Connection, options: &Options, checks: &mut Vec<Check>) {
    let mut check = Check::new("V16", "备份留痕且文件在位");
    let total = scalar_i64(conn, "SELECT COUNT(*) FROM backups");
    check.note(format!("备份 {total} 份"));
    if total == 0 {
        check.skip("尚无备份，先在界面创建一份");
        checks.push(check);
        return;
    }
    let missing = scalar_i64(conn, "SELECT COUNT(*) FROM backups WHERE present = 1 AND checksum = ''");
    if missing > 0 {
        check.fail(format!("{missing} 份备份未记录校验值"));
    }

    // 逐个确认文件还在，并给出可核对的信息。
    let mut stmt = match conn.prepare("SELECT path, size_bytes, kind, created_at FROM backups ORDER BY created_at DESC") {
        Ok(stmt) => stmt,
        Err(error) => {
            check.fail(error.to_string());
            checks.push(check);
            return;
        }
    };
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    });
    let mut absent = 0;
    let mut pre_migration = 0;
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            let (path, size, kind, created) = row;
            if kind == "pre_migration" {
                pre_migration += 1;
            }
            let on_disk = Path::new(&path).is_file();
            if !on_disk {
                absent += 1;
            }
            check.note(format!(
                "{created} {kind} {size} 字节 文件{}：{path}",
                if on_disk { "在" } else { "缺" }
            ));
        }
    }
    if absent > 0 {
        check.fail(format!("{absent} 份备份的文件不在了"));
    }
    check.note(format!("迁移前自动备份 {pre_migration} 份"));
    if options.expect_pre_migration && pre_migration == 0 {
        check.fail("要求存在迁移前自动备份，但没有");
    }
    checks.push(check);
}

/// V18：外部来源的注入标记与快照。
fn check_external_sources(conn: &Connection, options: &Options, checks: &mut Vec<Check>) {
    let mut check = Check::new("V18", "外部内容隔离标记");
    let total = scalar_i64(conn, "SELECT COUNT(*) FROM council_sources");
    let flagged = scalar_i64(conn, "SELECT COUNT(*) FROM council_sources WHERE flagged = 1");
    check.note(format!("检索快照 {total} 条，其中标记为可疑指令 {flagged} 条"));
    if total == 0 {
        check.skip("尚无检索快照，先跑一次带检索的会诊");
        checks.push(check);
        return;
    }
    if options.expect_flagged && flagged == 0 {
        check.fail("要求存在被标记的外部来源，但一条也没有");
    }
    // 标记为可疑的来源必须留下可见内容，否则界面无从提示。
    let empty = scalar_i64(
        conn,
        "SELECT COUNT(*) FROM council_sources WHERE flagged = 1 AND TRIM(snippet) = ''",
    );
    if empty > 0 {
        check.fail(format!("{empty} 条被标记的来源没有摘要内容"));
    }
    let bodies = scalar_i64(
        conn,
        "SELECT COUNT(*) FROM council_sources WHERE body IS NOT NULL AND body != ''",
    );
    check.note(format!("带正文快照 {bodies} 条（connector.snapshot_body 开启时才有）"));
    checks.push(check);
}

/// 联网总开关与平台配置，V3 的前置条件。
fn check_network_ready(conn: &Connection, checks: &mut Vec<Check>) {
    let mut check = Check::new("S2", "联网开关与平台配置");
    let enabled = scalar_text(conn, "SELECT value FROM settings WHERE key = 'networking_enabled'");
    check.note(format!(
        "联网总开关：{}",
        enabled.unwrap_or_else(|| "未设置（默认关闭）".to_string())
    ));
    let platforms = scalar_i64(conn, "SELECT COUNT(*) FROM ai_platforms");
    let ready = scalar_i64(
        conn,
        "SELECT COUNT(*) FROM ai_platforms WHERE enabled = 1",
    );
    check.note(format!("平台 {platforms} 个，启用 {ready} 个"));
    let refs = scalar_i64(conn, "SELECT COUNT(*) FROM credential_refs");
    check.note(format!("凭据引用 {refs} 条（只存引用名，密钥在系统凭据库）"));
    if platforms > 0 && ready == 0 {
        // 不算失败：V3 需要先启用平台，这里只提示。
        check.note("提示：有平台但未启用，V3 探针会返回「平台未配置」");
    }
    checks.push(check);
}

fn main() -> ExitCode {
    let options = parse_args();
    if !options.db.is_file() {
        eprintln!("找不到数据库文件：{}", options.db.display());
        return ExitCode::from(2);
    }
    let conn = match open_read_only(&options.db) {
        Ok(conn) => conn,
        Err(error) => {
            eprintln!("无法只读打开数据库：{error}");
            return ExitCode::from(2);
        }
    };

    let mut checks = Vec::new();
    check_schema(&conn, &mut checks);
    check_network_ready(&conn, &mut checks);
    check_secret(&conn, &options, &mut checks);
    check_llm_audit(&conn, &mut checks);
    check_capture(&conn, &options, &mut checks);
    check_search(&conn, &options, &mut checks);
    check_query_redaction(&conn, &mut checks);
    check_backups(&conn, &options, &mut checks);
    check_external_sources(&conn, &options, &mut checks);

    println!("思想熔炉 · 真机验证只读检查");
    println!("数据库：{}", options.db.display());
    println!();
    for check in &checks {
        println!("[{}] {} {}", check.verdict.tag(), check.id, check.title);
        for line in &check.lines {
            println!("      {line}");
        }
    }

    let failed = checks
        .iter()
        .filter(|check| check.verdict == Verdict::Fail)
        .count();
    let skipped = checks
        .iter()
        .filter(|check| check.verdict == Verdict::Skip)
        .count();
    println!();
    println!(
        "合计 {} 项：通过 {}，未过 {}，跳过 {}",
        checks.len(),
        checks.len() - failed - skipped,
        failed,
        skipped
    );
    if failed > 0 {
        println!("请把未过项与跳过原因记入验证记录表。");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
