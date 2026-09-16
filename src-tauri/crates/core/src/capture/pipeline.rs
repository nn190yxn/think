//! 采集写入链路：采样合并、脱敏、哈希去重与批量落库。
//!
//! 单 worker 语义由调用方保证：一轮 `collect_once` 内先取回全部样本，
//! 再在一个事务里写入，中途不对外暴露半成品。

use std::collections::HashSet;

use rusqlite::Connection;

use crate::error::{CoreError, CoreResult};
use crate::util::{sha256_hex, unique_id};

use super::redact::redact_value;
use super::repo;
use super::{
    parse_epoch, CaptureCapabilityView, CaptureFilter, CaptureKind, CaptureOutcome,
    CaptureSettingsView, CaptureSource, RawSample, CAPTURE_KINDS, FILE_QUEUE_CAPACITY,
    FILE_TRAILING_WINDOW_SECONDS, KIND_FILE, MAX_WATCH_ROOTS, WINDOW_MERGE_GAP_SECONDS,
};

/// 采集能力视图。`unavailable` 里的能力由外壳标记为系统不可用。
pub fn settings_view(conn: &Connection, unavailable: &[&str]) -> CoreResult<CaptureSettingsView> {
    let mut capabilities = Vec::with_capacity(CAPTURE_KINDS.len());
    for kind in CAPTURE_KINDS {
        let (enabled, consented_at) = repo::capability(conn, kind)?;
        let label = CaptureKind::parse(kind)
            .map(|value| value.label())
            .unwrap_or(kind);
        capabilities.push(CaptureCapabilityView {
            kind: kind.to_string(),
            label: label.to_string(),
            enabled,
            available: !unavailable.contains(&kind),
            consented_at,
        });
    }
    let rules = repo::redaction_rules(conn)?;
    Ok(CaptureSettingsView {
        paused: repo::paused(conn)?,
        dedup_seconds: repo::dedup_seconds(conn)?,
        redaction_enabled: rules.enabled,
        redaction_terms: rules.terms.len() as i64,
        capabilities,
        watch_roots: repo::watch_roots(conn)?,
    })
}

/// 逐条校验并归一化受关注目录，返回接受的路径与第一条错误说明。
///
/// 只接受已存在的绝对路径：相对路径的含义随进程工作目录变化，静默放行会让用户
/// 以为监听生效了而实际盯着别处。嵌套在已接受目录内的路径会被丢弃，否则 notify
/// 会对同一个文件重复上报。
fn vet_watch_roots(paths: &[String]) -> (Vec<String>, Option<String>) {
    let mut accepted: Vec<std::path::PathBuf> = Vec::new();
    let mut failure = None;
    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = std::path::PathBuf::from(trimmed);
        if !path.is_absolute() {
            failure.get_or_insert_with(|| format!("关注目录需要绝对路径：{trimmed}"));
            continue;
        }
        if !path.is_dir() {
            failure.get_or_insert_with(|| format!("目录不存在或不可读：{trimmed}"));
            continue;
        }
        if accepted.iter().any(|existing| path.starts_with(existing)) {
            continue;
        }
        accepted.retain(|existing| !existing.starts_with(&path));
        accepted.push(path);
    }
    if accepted.len() > MAX_WATCH_ROOTS {
        failure.get_or_insert_with(|| format!("关注目录最多 {MAX_WATCH_ROOTS} 个"));
        accepted.truncate(MAX_WATCH_ROOTS);
    }
    let accepted = accepted
        .into_iter()
        .map(|path| path.display().to_string())
        .collect();
    (accepted, failure)
}

/// 界面提交用的严格校验：任何一条不合格都拒绝整次提交，让用户看到原因。
pub fn normalize_watch_roots(paths: &[String]) -> CoreResult<Vec<String>> {
    let (accepted, failure) = vet_watch_roots(paths);
    match failure {
        Some(message) => Err(CoreError::InvalidInput(message)),
        None => Ok(accepted),
    }
}

/// 启动时读取设置用的宽松校验：丢弃坏项，保留其余可用目录。
pub fn accepted_watch_roots(paths: &[String]) -> Vec<String> {
    vet_watch_roots(paths).0
}

/// 保存受关注目录并写审计。外壳在这之后按返回值重建文件监听。
pub fn set_watch_roots(
    conn: &Connection,
    paths: &[String],
    unavailable: &[&str],
) -> CoreResult<CaptureSettingsView> {
    let roots = normalize_watch_roots(paths)?;
    repo::set_watch_roots(conn, &roots)?;
    if !roots.is_empty() {
        repo::insert_audit(
            conn,
            &unique_id("cap-audit", &format!("watch-roots{}", roots.len())),
            KIND_FILE,
            "watch_roots",
            &format!("监听 {} 个目录", roots.len()),
        )?;
    }
    settings_view(conn, unavailable)
}

/// 切换单项能力并写审计。
pub fn set_capability(
    conn: &Connection,
    kind: &str,
    enabled: bool,
    unavailable: &[&str],
) -> CoreResult<CaptureSettingsView> {
    let parsed = CaptureKind::parse(kind)
        .ok_or_else(|| crate::error::CoreError::InvalidInput(format!("未知采集能力：{kind}")))?;
    if enabled && unavailable.contains(&kind) {
        return Err(crate::error::CoreError::InvalidInput(format!(
            "{} 在当前系统上不可用",
            parsed.label()
        )));
    }
    repo::set_capability(conn, kind, enabled)?;
    repo::insert_audit(
        conn,
        &unique_id("cap-audit", &format!("{kind}{enabled}")),
        kind,
        if enabled { "enable" } else { "disable" },
        "",
    )?;
    settings_view(conn, unavailable)
}

/// 全局暂停与恢复，均写审计。
pub fn set_paused(
    conn: &Connection,
    paused: bool,
    unavailable: &[&str],
) -> CoreResult<CaptureSettingsView> {
    repo::set_paused(conn, paused)?;
    repo::insert_audit(
        conn,
        &unique_id("cap-audit", &format!("pause{paused}")),
        "global",
        if paused { "pause" } else { "resume" },
        "",
    )?;
    settings_view(conn, unavailable)
}

/// 一轮采集：轮询、合并、脱敏、去重、批量落库。
///
/// 暂停时直接返回且不轮询、不写入；采样源报错时只计入错误计数。
pub fn collect_once(
    conn: &mut Connection,
    source: &dyn CaptureSource,
    occurred_at: &str,
) -> CoreResult<CaptureOutcome> {
    if repo::paused(conn)? {
        return Ok(CaptureOutcome {
            paused: true,
            ..CaptureOutcome::default()
        });
    }

    let samples = match source.poll() {
        Ok(samples) => samples,
        Err(_) => {
            return Ok(CaptureOutcome {
                errors: 1,
                ..CaptureOutcome::default()
            })
        }
    };

    let mut dropped = 0i64;
    let mut file_count = 0usize;
    let mut bounded = Vec::with_capacity(samples.len());
    for sample in samples {
        if sample.kind == CaptureKind::File {
            file_count += 1;
            if file_count > FILE_QUEUE_CAPACITY {
                dropped += 1;
                continue;
            }
        }
        bounded.push(sample);
    }

    let merged = merge_file_samples(merge_window_samples(bounded));
    let mut outcome = ingest_samples(conn, merged, occurred_at)?;
    outcome.dropped = dropped;
    Ok(outcome)
}

/// 直接写入一批样本，用于测试与外壳批量注入。
pub fn ingest_samples(
    conn: &mut Connection,
    samples: Vec<RawSample>,
    occurred_at: &str,
) -> CoreResult<CaptureOutcome> {
    if repo::paused(conn)? {
        return Ok(CaptureOutcome {
            paused: true,
            ..CaptureOutcome::default()
        });
    }

    let rules = repo::redaction_rules(conn)?;
    let dedup = repo::dedup_seconds(conn)?;
    let created_at = repo::now(conn)?;
    let mut enabled = Vec::with_capacity(CAPTURE_KINDS.len());
    for kind in CAPTURE_KINDS {
        enabled.push((kind, repo::capability(conn, kind)?.0));
    }

    let mut outcome = CaptureOutcome {
        polled: samples.len() as i64,
        ..CaptureOutcome::default()
    };
    let mut seen: HashSet<(String, String)> = HashSet::new();

    let tx = conn.transaction()?;
    for sample in samples {
        let kind = sample.kind.as_str();
        if !enabled.iter().any(|(key, on)| *key == kind && *on) {
            outcome.skipped_disabled += 1;
            continue;
        }

        let payload = build_payload(&sample);
        let (payload, redacted, _hits) = redact_value(&payload, &rules);
        let canonical = serde_json::to_string(&payload).unwrap_or_default();
        let content_hash = sha256_hex(&canonical);
        let key = (kind.to_string(), content_hash.clone());
        if seen.contains(&key)
            || repo::hash_seen(&tx, kind, &content_hash, &sample.occurred_at, dedup)?
        {
            outcome.skipped_duplicate += 1;
            continue;
        }
        seen.insert(key);

        let id = unique_id("cap", &format!("{kind}{content_hash}{}", sample.occurred_at));
        let (topic, excerpt) = derive_summary(&sample, &payload);
        let occurred_at = if sample.occurred_at.is_empty() {
            occurred_at.to_string()
        } else {
            sample.occurred_at.clone()
        };
        repo::insert_event(
            &tx,
            &id,
            kind,
            &occurred_at,
            &sample.source_app,
            &canonical,
            &content_hash,
            redacted,
            &created_at,
        )?;
        repo::insert_summary(
            &tx,
            &unique_id("cap-sum", &id),
            &id,
            &topic,
            &excerpt,
            &created_at,
        )?;
        outcome.written += 1;
        if redacted {
            outcome.redacted += 1;
        }
    }
    tx.commit()?;
    Ok(outcome)
}

/// 剪贴板/窗口/文件记录的读取与删除。
pub fn list_events(conn: &Connection, filter: &CaptureFilter) -> CoreResult<Vec<super::CaptureEventView>> {
    repo::list_events(conn, filter)
}

pub fn summaries_of(conn: &Connection, event_id: &str) -> CoreResult<Vec<super::CaptureSummaryView>> {
    repo::summaries_of(conn, event_id)
}

pub fn delete_event(conn: &Connection, id: &str) -> CoreResult<bool> {
    repo::delete_event(conn, id)
}

pub fn list_audit(conn: &Connection, limit: i64) -> CoreResult<Vec<super::CaptureAuditView>> {
    repo::list_audit(conn, limit)
}

/// 合并同一窗口的连续采样为一段，时长累加。
pub fn merge_window_samples(samples: Vec<RawSample>) -> Vec<RawSample> {
    let mut out: Vec<RawSample> = Vec::with_capacity(samples.len());
    for sample in samples {
        if sample.kind != CaptureKind::Window {
            out.push(sample);
            continue;
        }
        let app = payload_string(&sample.payload, "app");
        let title = payload_string(&sample.payload, "title");
        let duration = payload_number(&sample.payload, "durationMs");
        let can_merge = out.last().map(|last| {
            last.kind == CaptureKind::Window
                && payload_string(&last.payload, "app") == app
                && payload_string(&last.payload, "title") == title
                && within_gap(&last.occurred_at, &sample.occurred_at, WINDOW_MERGE_GAP_SECONDS)
        });
        if can_merge == Some(true) {
            if let Some(last) = out.last_mut() {
                let total = payload_number(&last.payload, "durationMs") + duration;
                last.payload["durationMs"] = serde_json::json!(total);
            }
        } else {
            out.push(sample);
        }
    }
    out
}

/// 文件活动的 trailing window 合并：窗口内同一路径只留一条，事件类型取最后一次。
pub fn merge_file_samples(samples: Vec<RawSample>) -> Vec<RawSample> {
    let mut out: Vec<RawSample> = Vec::with_capacity(samples.len());
    for sample in samples {
        if sample.kind != CaptureKind::File {
            out.push(sample);
            continue;
        }
        let path = payload_string(&sample.payload, "path");
        let can_merge = out.last().map(|last| {
            last.kind == CaptureKind::File
                && payload_string(&last.payload, "path") == path
                && within_gap(
                    &last.occurred_at,
                    &sample.occurred_at,
                    FILE_TRAILING_WINDOW_SECONDS,
                )
        });
        if can_merge == Some(true) {
            if let Some(last) = out.last_mut() {
                last.payload["eventType"] = sample.payload["eventType"].clone();
                let count = payload_number(&last.payload, "count") + 1;
                last.payload["count"] = serde_json::json!(count);
            }
        } else {
            let mut sample = sample;
            let mut payload = if sample.payload.is_object() {
                sample.payload.clone()
            } else {
                serde_json::json!({})
            };
            if payload_number(&payload, "count") == 0 {
                payload["count"] = serde_json::json!(1);
            }
            sample.payload = payload;
            out.push(sample);
        }
    }
    out
}

fn build_payload(sample: &RawSample) -> serde_json::Value {
    match sample.kind {
        CaptureKind::ClipboardText => {
            serde_json::json!({ "text": super::normalize_text(&sample.text) })
        }
        CaptureKind::ClipboardImage => {
            if sample.payload.is_object() {
                sample.payload.clone()
            } else {
                serde_json::json!({ "imageRef": super::normalize_text(&sample.text) })
            }
        }
        CaptureKind::Window => {
            if sample.payload.is_object() {
                sample.payload.clone()
            } else {
                serde_json::json!({
                    "app": sample.source_app,
                    "title": super::normalize_text(&sample.text),
                    "durationMs": 0,
                })
            }
        }
        CaptureKind::File => {
            if sample.payload.is_object() {
                sample.payload.clone()
            } else {
                serde_json::json!({ "path": sample.text, "eventType": "change", "count": 1 })
            }
        }
    }
}

fn derive_summary(sample: &RawSample, payload: &serde_json::Value) -> (String, String) {
    let topic = match sample.kind {
        CaptureKind::ClipboardText => "剪贴板文本".to_string(),
        CaptureKind::ClipboardImage => "剪贴板图片".to_string(),
        CaptureKind::Window => {
            if sample.source_app.is_empty() {
                "前台窗口".to_string()
            } else {
                sample.source_app.clone()
            }
        }
        CaptureKind::File => "文件活动".to_string(),
    };
    let excerpt = match sample.kind {
        CaptureKind::ClipboardText => payload_string(payload, "text"),
        CaptureKind::ClipboardImage => payload_string(payload, "imageRef"),
        CaptureKind::Window => payload_string(payload, "title"),
        CaptureKind::File => payload_string(payload, "path"),
    };
    (topic, excerpt)
}

fn payload_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string()
}

fn payload_number(value: &serde_json::Value, key: &str) -> i64 {
    value.get(key).and_then(|item| item.as_i64()).unwrap_or(0)
}

fn within_gap(previous: &str, current: &str, gap_seconds: i64) -> bool {
    match (parse_epoch(previous), parse_epoch(current)) {
        (Some(a), Some(b)) => (b - a).abs() <= gap_seconds,
        _ => false,
    }
}
