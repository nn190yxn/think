//! 资产服务：Skill 目录扫描、清单解析与统计汇总。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;

use crate::error::CoreResult;
use crate::util::sha256_hex;

use super::repo::{self, SkillUpsert};
use super::{
    manifest_excerpt, parse_manifest, tags_json, AssetRootView, AssetScanOutcome, AssetSummary,
    SkillDetail, SkillFilter, SkillView, MANIFEST_JSON, MANIFEST_MARKDOWN,
};

struct SkillCandidate {
    dir: PathBuf,
    name_hint: String,
}

pub fn add_root(conn: &Connection, path: &str) -> CoreResult<AssetRootView> {
    repo::add_root(conn, path)
}

pub fn list_roots(conn: &Connection) -> CoreResult<Vec<AssetRootView>> {
    repo::list_roots(conn)
}

pub fn remove_root(conn: &Connection, id: &str) -> CoreResult<bool> {
    repo::remove_root(conn, id)
}

/// 扫描全部根目录，逐条返回结果。
pub fn scan_assets(
    conn: &mut Connection,
    root_id: Option<&str>,
) -> CoreResult<Vec<AssetScanOutcome>> {
    let roots = match root_id {
        Some(id) => vec![repo::get_root(conn, id)?],
        None => repo::list_roots(conn)?,
    };
    let mut outcomes = Vec::with_capacity(roots.len());
    for root in roots {
        outcomes.push(scan_root(conn, &root.id)?);
    }
    Ok(outcomes)
}

/// 扫描一个根目录。根目录离线时保留既有记录并标记不可读。
pub fn scan_root(conn: &mut Connection, root_id: &str) -> CoreResult<AssetScanOutcome> {
    let root = repo::get_root(conn, root_id)?;
    if !repo::path_available(&root.path) {
        repo::mark_root_skills_missing(conn, root_id, true)?;
        let outcome = AssetScanOutcome {
            root_id: root_id.to_string(),
            root_path: root.path.clone(),
            available: false,
            reason: Some("root_unavailable".to_string()),
            ..AssetScanOutcome::default()
        };
        repo::record_scan(conn, &outcome)?;
        return Ok(outcome);
    }

    let candidates = discover(Path::new(&root.path))?;
    let scanned = candidates.len() as i64;
    let mut added = 0i64;
    let mut updated = 0i64;
    let mut needs_repair = 0i64;
    let mut keep: HashSet<String> = HashSet::new();

    let tx = conn.transaction()?;
    for candidate in &candidates {
        let manifest = parse_manifest(&candidate.dir, &candidate.name_hint);
        let path = candidate.dir.to_string_lossy().to_string();
        let modified_at = modified_at(&candidate.dir);
        let content_hash = sha256_hex(&format!(
            "{}|{}|{}|{}|{}",
            path,
            manifest.name,
            manifest.description,
            manifest.category,
            modified_at.clone().unwrap_or_default()
        ));
        let is_new = repo::upsert_skill(
            &tx,
            &SkillUpsert {
                root_id,
                path: &path,
                name: &manifest.name,
                description: &manifest.description,
                category: &manifest.category,
                tags_json: &tags_json(&manifest.tags),
                enabled: manifest.enabled,
                source: &manifest.source,
                version: &manifest.version,
                needs_repair: manifest.needs_repair,
                repair_reason: &manifest.repair_reason,
                content_hash: &content_hash,
                modified_at: modified_at.as_deref(),
                dependencies: &manifest.dependencies,
            },
        )?;
        if is_new {
            added += 1;
        } else {
            updated += 1;
        }
        if manifest.needs_repair {
            needs_repair += 1;
        }
        keep.insert(path);
    }
    let removed = repo::remove_missing_skills(&tx, root_id, &keep)?;
    tx.commit()?;
    repo::mark_root_scanned(conn, root_id)?;

    let outcome = AssetScanOutcome {
        root_id: root_id.to_string(),
        root_path: root.path,
        available: true,
        scanned,
        added,
        updated,
        removed,
        needs_repair,
        reason: None,
    };
    repo::record_scan(conn, &outcome)?;
    Ok(outcome)
}

pub fn list_skills(conn: &Connection, filter: &SkillFilter) -> CoreResult<Vec<SkillView>> {
    repo::list_skills(conn, filter)
}

pub fn skill_detail(conn: &Connection, skill_id: &str) -> CoreResult<SkillDetail> {
    let skill = repo::get_skill(conn, skill_id)?;
    let dependencies = repo::dependencies(conn, skill_id)?;
    let excerpt = manifest_excerpt(Path::new(&skill.path), &skill.source);
    Ok(SkillDetail {
        skill,
        dependencies,
        manifest_excerpt: excerpt,
    })
}

/// 资产总览：数量、分类分布、启用比例、近 30 天变动与平台接入状态。
pub fn summary(conn: &Connection) -> CoreResult<AssetSummary> {
    let roots = repo::list_roots(conn)?;
    let skill_count = repo::skill_count(conn)?;
    let enabled_count = repo::enabled_count(conn)?;
    let (recent_added, recent_removed) = repo::recent_changes(conn)?;
    Ok(AssetSummary {
        root_count: roots.len() as i64,
        available_roots: roots.iter().filter(|root| root.available).count() as i64,
        skill_count,
        enabled_count,
        disabled_count: skill_count - enabled_count,
        needs_repair_count: repo::needs_repair_count(conn)?,
        recent_added,
        recent_removed,
        categories: repo::category_stats(conn)?,
        platforms: repo::platform_stats(conn)?,
    })
}

/// 发现根目录下的 Skill：一层子目录各算一个；根目录自身带清单且无子目录时，
/// 把根目录当作单个 Skill。
fn discover(root: &Path) -> CoreResult<Vec<SkillCandidate>> {
    let mut candidates = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return Ok(candidates),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }
        candidates.push(SkillCandidate {
            dir: entry.path(),
            name_hint: name,
        });
    }
    if candidates.is_empty() && has_manifest(root) {
        let name_hint = root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        candidates.push(SkillCandidate {
            dir: root.to_path_buf(),
            name_hint,
        });
    }
    candidates.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(candidates)
}

fn has_manifest(dir: &Path) -> bool {
    dir.join(MANIFEST_JSON).is_file() || dir.join(MANIFEST_MARKDOWN).is_file()
}

/// Skill 目录的最近改动时间，取目录自身的 mtime。
fn modified_at(dir: &Path) -> Option<String> {
    let metadata = fs::metadata(dir).ok()?;
    let time = metadata.modified().ok()?;
    let seconds = time.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(epoch_to_rfc3339(seconds))
}

fn epoch_to_rfc3339(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// 天数转公历年月日（Howard Hinnant 的 civil_from_days）。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}
