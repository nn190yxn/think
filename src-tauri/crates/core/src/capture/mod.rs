//! 行为采集：剪贴板、前台窗口与文件活动。
//!
//! 内核只负责归一化、脱敏、去重、合并与落库，操作系统级的抓取由桌面外壳
//! 通过 [`CaptureSource`] 注入，因此这一层可以在没有桌面依赖的环境下测试。

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;

pub mod pipeline;
pub mod redact;
pub mod repo;

pub use redact::RedactionRules;

pub const KIND_CLIPBOARD_TEXT: &str = "clipboard_text";
pub const KIND_CLIPBOARD_IMAGE: &str = "clipboard_image";
pub const KIND_WINDOW: &str = "window";
pub const KIND_FILE: &str = "file";

/// 四类采集能力，顺序与界面一致。
pub const CAPTURE_KINDS: [&str; 4] = [
    KIND_CLIPBOARD_TEXT,
    KIND_CLIPBOARD_IMAGE,
    KIND_WINDOW,
    KIND_FILE,
];

/// 剪贴板轮询间隔，800 毫秒足以覆盖人工复制节奏。
pub const CLIPBOARD_POLL_MS: i64 = 800;
/// 前台窗口采样间隔。
pub const WINDOW_POLL_MS: i64 = 2_000;
/// 同一窗口连续采样的合并间隔：间隔不超过该值视为一段。
pub const WINDOW_MERGE_GAP_SECONDS: i64 = 6;
/// 文件活动的 trailing window：窗口内同一路径的多次事件合并为一条。
pub const FILE_TRAILING_WINDOW_SECONDS: i64 = 5;
/// 文件活动队列上限，超出部分丢弃并计数，避免爆发式写入压垮内存。
pub const FILE_QUEUE_CAPACITY: usize = 1_000;
/// 剪贴板去重窗口：相同内容在其内只落一条。
pub const DEFAULT_DEDUP_SECONDS: i64 = 300;
/// 单条采集正文的长度上限，超出截断。
pub const MAX_TEXT_CHARS: usize = 4_000;
/// 派生摘要的摘录长度。
pub const MAX_EXCERPT_CHARS: usize = 120;

pub const PAUSE_SETTING_KEY: &str = "capture.paused";
pub const REDACTION_SETTING_KEY: &str = "capture.redaction";
/// 受关注目录列表，值为 JSON 字符串数组。外壳据此建立文件监听。
pub const WATCH_ROOTS_SETTING_KEY: &str = "capture.watch_roots";
/// 受关注目录数量上限，避免一次挂上过多监听。
pub const MAX_WATCH_ROOTS: usize = 16;

/// 采集事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    ClipboardText,
    ClipboardImage,
    Window,
    File,
}

impl CaptureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CaptureKind::ClipboardText => KIND_CLIPBOARD_TEXT,
            CaptureKind::ClipboardImage => KIND_CLIPBOARD_IMAGE,
            CaptureKind::Window => KIND_WINDOW,
            CaptureKind::File => KIND_FILE,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CaptureKind::ClipboardText => "剪贴板文本",
            CaptureKind::ClipboardImage => "剪贴板图片",
            CaptureKind::Window => "前台窗口",
            CaptureKind::File => "文件活动",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            KIND_CLIPBOARD_TEXT => Some(CaptureKind::ClipboardText),
            KIND_CLIPBOARD_IMAGE => Some(CaptureKind::ClipboardImage),
            KIND_WINDOW => Some(CaptureKind::Window),
            KIND_FILE => Some(CaptureKind::File),
            _ => None,
        }
    }
}

/// 采集能力状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureCapabilityView {
    pub kind: String,
    pub label: String,
    pub enabled: bool,
    /// 系统级可用性。内核默认可用，外壳在系统拒绝权限时置为不可用。
    pub available: bool,
    pub consented_at: Option<String>,
}

/// 采集总览：全局暂停、去重窗口、脱敏规则与四类能力。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSettingsView {
    pub paused: bool,
    pub dedup_seconds: i64,
    pub redaction_enabled: bool,
    pub redaction_terms: i64,
    pub capabilities: Vec<CaptureCapabilityView>,
    /// 当前生效的受关注目录。为空时文件活动不可用。
    pub watch_roots: Vec<String>,
}

/// 一条采集记录的对外视图。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureEventView {
    pub id: String,
    pub kind: String,
    pub occurred_at: String,
    pub source_app: String,
    pub payload: serde_json::Value,
    pub content_hash: String,
    pub redacted: bool,
    pub created_at: String,
}

/// 采集记录的派生摘要，随事件级联删除。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSummaryView {
    pub id: String,
    pub event_id: String,
    pub topic: String,
    pub excerpt: String,
    pub created_at: String,
}

/// 开启与暂停的审计记录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureAuditView {
    pub id: String,
    pub kind: String,
    pub action: String,
    pub reason: String,
    pub created_at: String,
}

/// 采集记录筛选条件。
#[derive(Debug, Clone, Default)]
pub struct CaptureFilter {
    pub kind: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

/// 一轮采集的统计结果。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOutcome {
    pub paused: bool,
    pub polled: i64,
    pub written: i64,
    pub skipped_disabled: i64,
    pub skipped_duplicate: i64,
    pub redacted: i64,
    pub dropped: i64,
    pub errors: i64,
}

/// 一条原始样本。正文只在这里短时存在，落库前先脱敏与归一化。
#[derive(Debug, Clone)]
pub struct RawSample {
    pub kind: CaptureKind,
    pub occurred_at: String,
    pub source_app: String,
    pub text: String,
    pub payload: serde_json::Value,
}

/// 操作系统级采集源。桌面外壳实现它，测试用固定脚本实现。
pub trait CaptureSource {
    fn poll(&self) -> CoreResult<Vec<RawSample>>;
}

/// 未接入采集源时的占位实现。
pub struct NoopCaptureSource;

impl CaptureSource for NoopCaptureSource {
    fn poll(&self) -> CoreResult<Vec<RawSample>> {
        Ok(Vec::new())
    }
}

/// 把 RFC3339（UTC，形如 `2026-09-14T10:00:00Z`）解析成 Unix 秒。
pub fn parse_epoch(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = value.get(0..4)?.parse().ok()?;
    let month: i64 = value.get(5..7)?.parse().ok()?;
    let day: i64 = value.get(8..10)?.parse().ok()?;
    let hour: i64 = value.get(11..13)?.parse().ok()?;
    let minute: i64 = value.get(14..16)?.parse().ok()?;
    let second: i64 = value.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // 以 1970-01-01 为基准按天累加，避免引入日期库。
    let days_before_year = |y: i64| -> i64 {
        let y = y - 1;
        y * 365 + y / 4 - y / 100 + y / 400
    };
    let leap = |y: i64| -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 };
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut days = days_before_year(year) - days_before_year(1970);
    days += month_days.iter().take(month as usize - 1).sum::<i64>();
    if month > 2 && leap(year) {
        days += 1;
    }
    days += day - 1;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// 单条正文的归一化：折叠空白并截断到上限。
pub fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_TEXT_CHARS));
    let mut last_space = false;
    for ch in text.chars() {
        let is_space = ch.is_whitespace();
        if is_space {
            if !last_space && !out.is_empty() {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
        last_space = is_space;
        if out.len() >= MAX_TEXT_CHARS {
            break;
        }
    }
    out.trim().to_string()
}
