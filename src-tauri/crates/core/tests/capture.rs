//! P7 采集：能力开关、去重、窗口/文件合并、脱敏、暂停与审计。

use std::cell::Cell;

use proptest::prelude::*;
use thought_forge_core::capture::pipeline::{self as capture_pipeline};
use thought_forge_core::capture::repo as capture_repo;
use thought_forge_core::capture::{
    CaptureFilter, CaptureKind, CaptureSource, RawSample, RedactionRules, CAPTURE_KINDS,
    DEFAULT_DEDUP_SECONDS, MAX_TEXT_CHARS,
};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::CoreResult;

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

/// 固定脚本采样源，记录被轮询次数。
struct ScriptedSource {
    samples: Vec<RawSample>,
    calls: Cell<u32>,
    fail: bool,
}

impl ScriptedSource {
    fn new(samples: Vec<RawSample>) -> Self {
        Self {
            samples,
            calls: Cell::new(0),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            samples: Vec::new(),
            calls: Cell::new(0),
            fail: true,
        }
    }
}

impl CaptureSource for ScriptedSource {
    fn poll(&self) -> CoreResult<Vec<RawSample>> {
        self.calls.set(self.calls.get() + 1);
        if self.fail {
            return Err(thought_forge_core::CoreError::Io(std::io::Error::other(
                "采集源不可用",
            )));
        }
        Ok(self.samples.clone())
    }
}

fn clipboard(text: &str, at: &str) -> RawSample {
    RawSample {
        kind: CaptureKind::ClipboardText,
        occurred_at: at.to_string(),
        source_app: "浏览器".to_string(),
        text: text.to_string(),
        payload: serde_json::Value::Null,
    }
}

fn window(app: &str, title: &str, duration_ms: i64, at: &str) -> RawSample {
    RawSample {
        kind: CaptureKind::Window,
        occurred_at: at.to_string(),
        source_app: app.to_string(),
        text: title.to_string(),
        payload: serde_json::json!({
            "app": app,
            "title": title,
            "durationMs": duration_ms,
        }),
    }
}

fn file(path: &str, event: &str, at: &str) -> RawSample {
    RawSample {
        kind: CaptureKind::File,
        occurred_at: at.to_string(),
        source_app: String::new(),
        text: path.to_string(),
        payload: serde_json::json!({ "path": path, "eventType": event, "count": 1 }),
    }
}

fn enable(conn: &rusqlite::Connection, kind: CaptureKind) {
    capture_pipeline::set_capability(conn, kind.as_str(), true, &[]).unwrap();
}

fn event_payload(conn: &rusqlite::Connection, index: usize) -> serde_json::Value {
    let events = capture_repo::list_events(conn, &CaptureFilter::default()).unwrap();
    events[index].payload.clone()
}

#[test]
fn capabilities_default_off_and_audit_each_toggle() {
    let conn = db();
    let view = capture_pipeline::settings_view(&conn, &[]).unwrap();
    assert_eq!(view.capabilities.len(), CAPTURE_KINDS.len());
    assert!(view.capabilities.iter().all(|item| !item.enabled));
    assert!(!view.paused);
    assert_eq!(view.dedup_seconds, DEFAULT_DEDUP_SECONDS);

    let after = capture_pipeline::set_capability(&conn, CaptureKind::Window.as_str(), true, &[]).unwrap();
    assert!(after
        .capabilities
        .iter()
        .find(|item| item.kind == CaptureKind::Window.as_str())
        .unwrap()
        .enabled);
    assert!(after
        .capabilities
        .iter()
        .find(|item| item.kind == CaptureKind::Window.as_str())
        .unwrap()
        .consented_at
        .is_some());

    capture_pipeline::set_capability(&conn, CaptureKind::Window.as_str(), false, &[]).unwrap();
    let audit = capture_pipeline::list_audit(&conn, 10).unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].action, "disable");
    assert_eq!(audit[1].action, "enable");
}

#[test]
fn unavailable_capability_cannot_be_enabled() {
    let conn = db();
    let error = capture_pipeline::set_capability(
        &conn,
        CaptureKind::ClipboardImage.as_str(),
        true,
        &[CaptureKind::ClipboardImage.as_str()],
    )
    .unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
    let view = capture_pipeline::settings_view(&conn, &[CaptureKind::ClipboardImage.as_str()]).unwrap();
    let capability = view
        .capabilities
        .iter()
        .find(|item| item.kind == CaptureKind::ClipboardImage.as_str())
        .unwrap();
    assert!(!capability.available);
    assert!(!capability.enabled);
}

#[test]
fn disabled_kind_is_skipped_and_not_written() {
    let mut conn = db();
    let source = ScriptedSource::new(vec![clipboard("待采纳的观点", "2026-09-14T10:00:00Z")]);
    let outcome = capture_pipeline::collect_once(&mut conn, &source, "2026-09-14T10:00:00Z").unwrap();
    assert_eq!(outcome.polled, 1);
    assert_eq!(outcome.skipped_disabled, 1);
    assert_eq!(outcome.written, 0);
    assert_eq!(capture_repo::count_events(&conn).unwrap(), 0);
}

#[test]
fn enabled_clipboard_writes_event_and_summary() {
    let mut conn = db();
    enable(&conn, CaptureKind::ClipboardText);
    let source = ScriptedSource::new(vec![clipboard("  注意力   是稀缺资源 ", "2026-09-14T10:00:00Z")]);
    let outcome = capture_pipeline::collect_once(&mut conn, &source, "2026-09-14T10:00:00Z").unwrap();
    assert_eq!(outcome.written, 1);
    assert_eq!(outcome.redacted, 0);

    let events = capture_repo::list_events(&conn, &CaptureFilter::default()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, CaptureKind::ClipboardText.as_str());
    assert_eq!(events[0].payload["text"], "注意力 是稀缺资源");

    let summaries = capture_repo::summaries_of(&conn, &events[0].id).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].excerpt, "注意力 是稀缺资源");
}

#[test]
fn repeated_content_is_deduplicated_within_window() {
    let mut conn = db();
    enable(&conn, CaptureKind::ClipboardText);
    let first = capture_pipeline::ingest_samples(
        &mut conn,
        vec![clipboard("同一段内容", "2026-09-14T10:00:00Z")],
        "2026-09-14T10:00:00Z",
    )
    .unwrap();
    assert_eq!(first.written, 1);

    let second = capture_pipeline::ingest_samples(
        &mut conn,
        vec![
            clipboard("同一段内容", "2026-09-14T10:01:00Z"),
            clipboard("同一段内容", "2026-09-14T10:01:30Z"),
        ],
        "2026-09-14T10:01:30Z",
    )
    .unwrap();
    assert_eq!(second.written, 0);
    assert_eq!(second.skipped_duplicate, 2);
    assert_eq!(capture_repo::count_events(&conn).unwrap(), 1);
}

#[test]
fn long_text_is_truncated_to_limit() {
    let mut conn = db();
    enable(&conn, CaptureKind::ClipboardText);
    let long = "甲".repeat(MAX_TEXT_CHARS + 500);
    capture_pipeline::ingest_samples(
        &mut conn,
        vec![clipboard(&long, "2026-09-14T10:00:00Z")],
        "2026-09-14T10:00:00Z",
    )
    .unwrap();
    let payload = event_payload(&conn, 0);
    let text = payload["text"].as_str().unwrap();
    assert!(text.chars().count() <= MAX_TEXT_CHARS);
}

#[test]
fn window_samples_merge_into_one_segment() {
    let mut conn = db();
    enable(&conn, CaptureKind::Window);
    let source = ScriptedSource::new(vec![
        window("编辑器", "设计文档", 1000, "2026-09-14T10:00:00Z"),
        window("编辑器", "设计文档", 2000, "2026-09-14T10:00:04Z"),
        window("编辑器", "设计文档", 500, "2026-09-14T10:00:40Z"),
    ]);
    capture_pipeline::collect_once(
        &mut conn,
        &source,
        "2026-09-14T10:00:40Z",
    )
    .unwrap();
    let events = capture_repo::list_events(&conn, &CaptureFilter::default()).unwrap();
    assert_eq!(events.len(), 2, "同窗口间隔内的采样应合并，超时后另起一段");
    let durations: Vec<i64> = events
        .iter()
        .map(|event| event.payload["durationMs"].as_i64().unwrap())
        .collect();
    assert!(durations.contains(&3000));
    assert!(durations.contains(&500));
}

#[test]
fn file_samples_merge_by_trailing_window() {
    let mut conn = db();
    enable(&conn, CaptureKind::File);
    let source = ScriptedSource::new(vec![
        file("D:/notes/a.md", "create", "2026-09-14T10:00:00Z"),
        file("D:/notes/a.md", "change", "2026-09-14T10:00:02Z"),
        file("D:/notes/b.md", "change", "2026-09-14T10:00:03Z"),
    ]);
    capture_pipeline::collect_once(
        &mut conn,
        &source,
        "2026-09-14T10:00:03Z",
    )
    .unwrap();
    let events = capture_repo::list_events(&conn, &CaptureFilter::default()).unwrap();
    assert_eq!(events.len(), 2);
    let merged = events
        .iter()
        .find(|event| event.payload["path"] == "D:/notes/a.md")
        .unwrap();
    assert_eq!(merged.payload["count"], 2);
    assert_eq!(merged.payload["eventType"], "change");
}

#[test]
fn redaction_masks_builtin_and_custom_terms() {
    let mut conn = db();
    enable(&conn, CaptureKind::ClipboardText);
    let rules = RedactionRules {
        enabled: true,
        terms: vec!["天河计划".to_string()],
        mask: "[已脱敏]".to_string(),
    };
    capture_repo::set_redaction_rules(&conn, &rules).unwrap();
    capture_pipeline::ingest_samples(
        &mut conn,
        vec![clipboard(
            "联系人 13812345678 a@b.com 归属天河计划",
            "2026-09-14T10:00:00Z",
        )],
        "2026-09-14T10:00:00Z",
    )
    .unwrap();
    let events = capture_repo::list_events(&conn, &CaptureFilter::default()).unwrap();
    assert!(events[0].redacted);
    let text = events[0].payload["text"].as_str().unwrap();
    assert!(!text.contains("13812345678"));
    assert!(!text.contains("a@b.com"));
    assert!(!text.contains("天河计划"));
}

#[test]
fn paused_collect_does_not_poll_or_write() {
    let mut conn = db();
    enable(&conn, CaptureKind::ClipboardText);
    let source = ScriptedSource::new(vec![clipboard("暂停期间的内容", "2026-09-14T10:00:00Z")]);
    capture_pipeline::set_paused(&conn, true, &[]).unwrap();

    let outcome = capture_pipeline::collect_once(&mut conn, &source, "2026-09-14T10:00:00Z").unwrap();
    assert!(outcome.paused);
    assert_eq!(source.calls.get(), 0, "暂停时不应轮询采集源");
    assert_eq!(capture_repo::count_events(&conn).unwrap(), 0);

    let injected = capture_pipeline::ingest_samples(
        &mut conn,
        vec![clipboard("直接注入也不应写入", "2026-09-14T10:00:00Z")],
        "2026-09-14T10:00:00Z",
    )
    .unwrap();
    assert!(injected.paused);
    assert_eq!(injected.written, 0);
    assert_eq!(capture_repo::count_events(&conn).unwrap(), 0);

    capture_pipeline::set_paused(&conn, false, &[]).unwrap();
    let resumed = capture_pipeline::collect_once(&mut conn, &source, "2026-09-14T10:00:00Z").unwrap();
    assert!(!resumed.paused);
    assert_eq!(resumed.written, 1);
}

#[test]
fn source_failure_is_counted_without_writing() {
    let mut conn = db();
    enable(&conn, CaptureKind::ClipboardText);
    let source = ScriptedSource::failing();
    let outcome = capture_pipeline::collect_once(&mut conn, &source, "2026-09-14T10:00:00Z").unwrap();
    assert_eq!(outcome.errors, 1);
    assert_eq!(outcome.written, 0);
    assert_eq!(capture_repo::count_events(&conn).unwrap(), 0);
}

#[test]
fn deleting_event_removes_summary() {
    let mut conn = db();
    enable(&conn, CaptureKind::ClipboardText);
    capture_pipeline::ingest_samples(
        &mut conn,
        vec![clipboard("会被删除的记录", "2026-09-14T10:00:00Z")],
        "2026-09-14T10:00:00Z",
    )
    .unwrap();
    let events = capture_repo::list_events(&conn, &CaptureFilter::default()).unwrap();
    let id = events[0].id.clone();
    assert!(capture_pipeline::delete_event(&conn, &id).unwrap());
    assert!(capture_repo::summaries_of(&conn, &id).unwrap().is_empty());
    assert_eq!(capture_repo::count_events(&conn).unwrap(), 0);
}

#[test]
fn file_queue_capacity_drops_overflow() {
    let mut conn = db();
    enable(&conn, CaptureKind::File);
    let samples: Vec<RawSample> = (0..1200)
        .map(|index| {
            file(
                &format!("D:/notes/{index}.md"),
                "change",
                "2026-09-14T10:00:00Z",
            )
        })
        .collect();
    let outcome =
        capture_pipeline::collect_once(&mut conn, &ScriptedSource::new(samples), "2026-09-14T10:00:00Z")
            .unwrap();
    assert_eq!(outcome.dropped, 200);
    assert_eq!(outcome.written, 1000);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// 属性：同一批内容重复注入不会产生重复记录。
    #[test]
    fn property_repeated_ingest_is_idempotent(content in "[^\\s]{1,40}") {
        let mut conn = db();
        enable(&conn, CaptureKind::ClipboardText);
        let batch = || vec![clipboard(&content, "2026-09-14T10:00:00Z")];

        let first = capture_pipeline::ingest_samples(&mut conn, batch(), "2026-09-14T10:00:00Z").unwrap();
        let second = capture_pipeline::ingest_samples(&mut conn, batch(), "2026-09-14T10:00:01Z").unwrap();
        prop_assert_eq!(first.written, 1);
        prop_assert_eq!(second.written, 0);
        prop_assert_eq!(second.skipped_duplicate, 1);
        prop_assert_eq!(capture_repo::count_events(&conn).unwrap(), 1);
    }

    /// 属性：全局暂停期间不轮询、不写入。
    #[test]
    fn property_paused_never_writes(content in ".{1,40}") {
        let mut conn = db();
        enable(&conn, CaptureKind::ClipboardText);
        capture_pipeline::set_paused(&conn, true, &[]).unwrap();
        let source = ScriptedSource::new(vec![clipboard(&content, "2026-09-14T10:00:00Z")]);

        let outcome = capture_pipeline::collect_once(&mut conn, &source, "2026-09-14T10:00:00Z").unwrap();
        prop_assert!(outcome.paused);
        prop_assert_eq!(source.calls.get(), 0);
        prop_assert_eq!(capture_repo::count_events(&conn).unwrap(), 0);
    }

    /// 属性：开启脱敏后，命中的数字串不会出现在落库正文里。
    #[test]
    fn property_redaction_removes_digits(digits in "[0-9]{11,15}") {
        let mut conn = db();
        enable(&conn, CaptureKind::ClipboardText);
        capture_repo::set_redaction_rules(&conn, &RedactionRules::default()).unwrap();
        let text = format!("订单号 {digits} 已确认");
        capture_pipeline::ingest_samples(
            &mut conn,
            vec![clipboard(&text, "2026-09-14T10:00:00Z")],
            "2026-09-14T10:00:00Z",
        )
        .unwrap();
        let events = capture_repo::list_events(&conn, &CaptureFilter::default()).unwrap();
        prop_assert_eq!(events.len(), 1);
        prop_assert!(events[0].redacted);
        let stored = events[0].payload["text"].as_str().unwrap();
        prop_assert!(!stored.contains(&digits));
    }
}
