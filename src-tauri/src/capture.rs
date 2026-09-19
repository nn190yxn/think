//! 桌面外壳的系统级采集源。
//!
//! 三类来源：剪贴板（文本与图片引用）、前台窗口、受关注目录的文件活动。
//! 剪贴板与前台窗口各有最小采样间隔，`poll` 只在间隔到达时取值；文件活动由
//! 常驻监听线程入队，`poll` 排空队列。脱敏、去重、合并与落库都由内核负责，
//! 这里只产出原始样本。
//!
//! 不可用的能力通过 [`ShellCapture::unavailable`] 上报，界面据此禁用开关，
//! 而不是让用户打开一个永远采不到数据的开关。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use thought_forge_core::capture::{
    pipeline as capture_pipeline,
    CaptureKind, CaptureSource, RawSample, FILE_QUEUE_CAPACITY, KIND_CLIPBOARD_IMAGE,
    KIND_CLIPBOARD_TEXT, KIND_FILE, KIND_WINDOW, WATCH_ROOTS_SETTING_KEY,
};
use thought_forge_core::{CoreError, CoreResult};

/// 剪贴板最小采样间隔，与设计一致。
pub const CLIPBOARD_INTERVAL_MS: u64 = 800;
/// 前台窗口最小采样间隔，与设计一致。
pub const WINDOW_INTERVAL_MS: u64 = 2000;
/// 关注目录列表的设置键，与内核共用同一个契约。
pub const WATCH_ROOTS_KEY: &str = WATCH_ROOTS_SETTING_KEY;

#[cfg(windows)]
use crate::capture_win as platform;

/// 非 Windows 平台上三类系统探针都不存在，统一返回空，避免上层写条件编译。
#[cfg(not(windows))]
mod platform {
    pub fn clipboard_text() -> Option<String> {
        None
    }

    pub fn clipboard_image_ref() -> Option<String> {
        None
    }

    pub fn foreground_window() -> Option<(String, String)> {
        None
    }
}

/// Unix 秒转 RFC3339 UTC（`2026-09-16T12:00:00Z`）。
fn format_utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// 以 1970-01-01 为第 0 天的日序转年月日，Howard Hinnant 的 `civil_from_days`。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // 把纪元挪到 0000-03-01，让闰年落在周期末尾。
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// notify 事件类型映射成样本里的 `eventType`。
fn classify(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Create(_) => "create",
        EventKind::Modify(_) => "modify",
        EventKind::Remove(_) => "remove",
        EventKind::Access(_) => "access",
        _ => "other",
    }
}

/// 队列里的一条文件活动：(路径, 事件类型, 发生时刻)。
type FileEntry = (String, String, i64);

/// 常驻文件活动监听。关注目录为空时不建立监听。
struct FileWatch {
    _watcher: Option<RecommendedWatcher>,
    roots: Vec<PathBuf>,
    queue: std::sync::Arc<Mutex<VecDeque<FileEntry>>>,
}

impl FileWatch {
    fn new(roots: &[PathBuf]) -> CoreResult<Self> {
        let queue = std::sync::Arc::new(Mutex::new(VecDeque::new()));
        if roots.is_empty() {
            return Ok(Self {
                _watcher: None,
                roots: Vec::new(),
                queue,
            });
        }
        let sink = std::sync::Arc::clone(&queue);
        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<Event>| {
                let Ok(event) = event else {
                    return;
                };
                let event_type = classify(&event.kind);
                let occurred = now_epoch();
                let Ok(mut entries) = sink.lock() else {
                    return;
                };
                for path in event.paths {
                    let Some(path) = path.to_str() else {
                        continue;
                    };
                    // 长时间不采集时只保留最近的若干条，避免队列无界增长。
                    if entries.len() >= FILE_QUEUE_CAPACITY {
                        entries.pop_front();
                    }
                    entries.push_back((path.to_string(), event_type.to_string(), occurred));
                }
            },
            Config::default(),
        )
        .map_err(|error| CoreError::InvalidInput(format!("建立文件监听失败：{error}")))?;
        for root in roots {
            watcher
                .watch(root, RecursiveMode::Recursive)
                .map_err(|error| {
                    CoreError::InvalidInput(format!("监听目录 {} 失败：{error}", root.display()))
                })?;
        }
        Ok(Self {
            _watcher: Some(watcher),
            roots: roots.to_vec(),
            queue,
        })
    }

    fn drain(&self) -> Vec<FileEntry> {
        match self.queue.lock() {
            Ok(mut entries) => entries.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// 采样节奏与最近一次取值，避免前端频繁触发时重复采样。
#[derive(Default)]
struct Cadence {
    last_clipboard: Option<Instant>,
    last_window: Option<Instant>,
    /// 上一次发出的剪贴板内容，用来跳过内容未变的轮次。
    last_clipboard_text: Option<String>,
}

impl Cadence {
    fn due(slot: &mut Option<Instant>, interval_ms: u64) -> bool {
        let now = Instant::now();
        let ready = slot
            .map(|last| now.duration_since(last).as_millis() as u64 >= interval_ms)
            .unwrap_or(true);
        if ready {
            *slot = Some(now);
        }
        ready
    }
}

/// 外壳采集源。常驻在应用状态里，文件监听因此可以跨多次采集持续积累事件。
pub struct ShellCapture {
    /// 文件监听可被替换，因此连同一份可变状态放进互斥量；`poll` 只在这里做短暂加锁。
    files: Mutex<FileWatch>,
    cadence: Mutex<Cadence>,
}

impl ShellCapture {
    pub fn new(roots: Vec<PathBuf>) -> CoreResult<Self> {
        Ok(Self {
            files: Mutex::new(FileWatch::new(&roots)?),
            cadence: Mutex::new(Cadence::default()),
        })
    }

    /// 替换受关注目录。先建成新监听再换入，失败时保持原有监听继续工作。
    pub fn set_watch_roots(&self, roots: Vec<PathBuf>) -> CoreResult<()> {
        // 建监听会做文件系统调用，放在锁外完成，避免阻塞正在进行的采集。
        let rebuilt = FileWatch::new(&roots)?;
        let Ok(mut files) = self.files.lock() else {
            return Err(CoreError::InvalidInput("文件监听锁不可用".to_string()));
        };
        *files = rebuilt;
        Ok(())
    }

    /// 未在本机或当前配置下提供的能力。界面据此禁用对应开关。
    pub fn unavailable(&self) -> Vec<&'static str> {
        let mut unavailable = Vec::new();
        if cfg!(not(windows)) {
            // 剪贴板与前台窗口依赖 Windows 原生接口。
            unavailable.push(KIND_CLIPBOARD_TEXT);
            unavailable.push(KIND_CLIPBOARD_IMAGE);
            unavailable.push(KIND_WINDOW);
        }
        let no_roots = match self.files.lock() {
            Ok(files) => files.roots.is_empty(),
            Err(_) => true,
        };
        if no_roots {
            unavailable.push(KIND_FILE);
        }
        unavailable
    }

    /// 从设置里读出关注目录，忽略不存在的路径并给出实际生效的列表。
    pub fn roots_from_setting(value: Option<&str>) -> Vec<PathBuf> {
        let Some(value) = value else {
            return Vec::new();
        };
        let parsed: Vec<String> = serde_json::from_str(value).unwrap_or_default();
        // 启动时容忍坏项：丢弃不合格的目录，而不是让整个应用起不来。
        capture_pipeline::accepted_watch_roots(&parsed)
            .into_iter()
            .map(PathBuf::from)
            .collect()
    }

    fn clipboard_samples(&self) -> Vec<RawSample> {
        let mut samples = Vec::new();
        if let Some(text) = platform::clipboard_text() {
            let changed = {
                let Ok(mut cadence) = self.cadence.lock() else {
                    return samples;
                };
                let changed = cadence.last_clipboard_text.as_deref() != Some(text.as_str());
                if changed {
                    cadence.last_clipboard_text = Some(text.clone());
                }
                changed
            };
            if changed {
                samples.push(RawSample {
                    kind: CaptureKind::ClipboardText,
                    occurred_at: format_utc(now_epoch()),
                    source_app: String::new(),
                    text,
                    payload: serde_json::Value::Null,
                });
            }
        }
        if let Some(reference) = platform::clipboard_image_ref() {
            samples.push(RawSample {
                kind: CaptureKind::ClipboardImage,
                occurred_at: format_utc(now_epoch()),
                source_app: String::new(),
                text: reference.clone(),
                payload: serde_json::json!({ "imageRef": reference }),
            });
        }
        samples
    }

    fn window_samples(&self) -> Vec<RawSample> {
        let Some((app, title)) = platform::foreground_window() else {
            return Vec::new();
        };
        vec![RawSample {
            kind: CaptureKind::Window,
            occurred_at: format_utc(now_epoch()),
            source_app: app.clone(),
            text: title.clone(),
            // 一次采样代表一个采样间隔的停留，由内核按窗口合并累加。
            payload: serde_json::json!({
                "app": app,
                "title": title,
                "durationMs": WINDOW_INTERVAL_MS,
            }),
        }]
    }

    fn file_samples(&self) -> Vec<RawSample> {
        let drained = match self.files.lock() {
            Ok(files) => files.drain(),
            Err(_) => Vec::new(),
        };
        drained
            .into_iter()
            .map(|(path, event_type, occurred)| RawSample {
                kind: CaptureKind::File,
                occurred_at: format_utc(occurred),
                source_app: String::new(),
                text: path.clone(),
                // 只记路径与事件类型，不读文件正文。
                payload: serde_json::json!({
                    "path": path,
                    "eventType": event_type,
                    "count": 1,
                }),
            })
            .collect()
    }
}

impl CaptureSource for ShellCapture {
    fn poll(&self) -> CoreResult<Vec<RawSample>> {
        let mut samples = Vec::new();
        {
            let Ok(mut cadence) = self.cadence.lock() else {
                return Err(CoreError::InvalidInput("采集节奏锁不可用".to_string()));
            };
            if Cadence::due(&mut cadence.last_clipboard, CLIPBOARD_INTERVAL_MS) {
                samples.extend(self.clipboard_samples());
            }
            if Cadence::due(&mut cadence.last_window, WINDOW_INTERVAL_MS) {
                samples.extend(self.window_samples());
            }
        }
        samples.extend(self.file_samples());
        Ok(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_format_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        // 2024-02-29 是闰日，验证闰年分支。
        assert_eq!(format_utc(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(format_utc(1_760_000_000), "2025-10-09T08:53:20Z");
        // 负时间戳不应崩溃，且仍落在正确的日期上。
        assert_eq!(format_utc(-86_400), "1969-12-31T00:00:00Z");
    }

    #[test]
    fn roots_setting_is_parsed_and_invalid_paths_dropped() {
        let existing = std::env::temp_dir();
        let value = serde_json::json!([
            existing.display().to_string(),
            "",
            "/definitely/not/a/real/directory/for/tests",
        ])
        .to_string();
        let roots = ShellCapture::roots_from_setting(Some(&value));
        assert_eq!(roots, vec![existing]);
        assert!(ShellCapture::roots_from_setting(None).is_empty());
        assert!(ShellCapture::roots_from_setting(Some("不是 JSON")).is_empty());
    }

    #[test]
    fn unavailable_reflects_platform_and_roots() {
        let without_roots = ShellCapture::new(Vec::new()).expect("建立采集源");
        let unavailable = without_roots.unavailable();
        assert!(unavailable.contains(&KIND_FILE));
        if cfg!(not(windows)) {
            assert!(unavailable.contains(&KIND_CLIPBOARD_TEXT));
            assert!(unavailable.contains(&KIND_WINDOW));
        }
    }

    #[test]
    fn file_watch_reports_created_file() {
        let root = std::env::temp_dir().join(format!(
            "tf-capture-{}-{}",
            std::process::id(),
            now_epoch()
        ));
        std::fs::create_dir_all(&root).expect("建立临时目录");
        let capture = ShellCapture::new(vec![root.clone()]).expect("建立采集源");

        // 监听由 notify 的独立线程投递，给事件留出送达时间。上限给到 10 秒：
        // 只用 2 秒时，若本轮构建/测试正把 CPU 占满，事件送达会被推迟而误报失败。
        std::fs::write(root.join("note.md"), "内容").expect("写入文件");
        let mut found = Vec::new();
        for _ in 0..200 {
            found = capture.file_samples();
            if !found.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        assert!(
            !found.is_empty(),
            "等了 10 秒仍未收到 {} 下的文件事件",
            root.display()
        );
        assert_eq!(found[0].kind, CaptureKind::File);
        assert!(found[0].text.ends_with("note.md"));
        let payload = &found[0].payload;
        assert_eq!(payload["eventType"], "create");
        // 正文不入库，负载里只有路径与事件类型。
        assert!(payload.get("content").is_none());
        // 时间戳落在可解析的 RFC3339 形状上，供内核做去重窗口计算。
        assert_eq!(found[0].occurred_at.len(), 20);
        assert!(found[0].occurred_at.ends_with('Z'));

        // 队列排空后不应重复上报同一条事件。判据不能取「队列为空」：Windows 上写
        // 一个文件会同时产生 create 与 modify 两个事件，投递时刻不定，稍晚到达的
        // modify 会让空队列断言随机失败。这里等一拍再排空，只要求同一路径上的同
        // 一个事件类型不再出现第二次。
        std::thread::sleep(std::time::Duration::from_millis(500));
        for sample in capture.file_samples() {
            assert!(
                !(sample.text == found[0].text
                    && sample.payload["eventType"] == found[0].payload["eventType"]),
                "同一条文件事件被重复上报：{}",
                sample.text
            );
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
