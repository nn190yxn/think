//! 资产仓储：根目录、Skill 索引、依赖与统计查询。

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, Transaction};

use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

use super::{
    category_label, tags_from_json_text, AssetRootView, CategoryStat, ManifestDependency,
    PlatformStat, SkillDependencyView, SkillFilter, SkillView, DEFAULT_SKILL_LIMIT,
    MAX_SKILL_LIMIT, RECENT_WINDOW_DAYS,
};

pub fn now(conn: &Connection) -> CoreResult<String> {
    let value: String = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [],
        |row| row.get(0),
    )?;
    Ok(value)
}

pub fn path_available(path: &str) -> bool {
    Path::new(path).is_dir()
}

// ---------- 根目录 ----------

fn read_root(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetRootView> {
    let path: String = row.get(1)?;
    Ok(AssetRootView {
        id: row.get(0)?,
        available: path_available(&path),
        path,
        last_scan_at: row.get(2)?,
        skill_count: row.get(3)?,
    })
}

const ROOT_COLUMNS: &str = "r.id, r.path, r.last_scan_at,
    (SELECT COUNT(*) FROM skills s WHERE s.root_id = r.id)";

pub fn list_roots(conn: &Connection) -> CoreResult<Vec<AssetRootView>> {
    let sql = format!("SELECT {ROOT_COLUMNS} FROM asset_roots r ORDER BY r.created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], read_root)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

pub fn get_root(conn: &Connection, id: &str) -> CoreResult<AssetRootView> {
    let sql = format!("SELECT {ROOT_COLUMNS} FROM asset_roots r WHERE r.id = ?1");
    conn.query_row(&sql, [id], read_root).map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => {
            CoreError::NotFound(format!("Skill 根目录不存在：{id}"))
        }
        other => other.into(),
    })
}

/// 登记根目录。路径重复时直接返回既有记录。
pub fn add_root(conn: &Connection, path: &str) -> CoreResult<AssetRootView> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("Skill 根目录路径不能为空".to_string()));
    }
    let mut stmt = conn.prepare("SELECT id FROM asset_roots WHERE path = ?1")?;
    let existing: Option<String> = stmt.query_row([trimmed], |row| row.get(0)).ok();
    drop(stmt);
    if let Some(id) = existing {
        return get_root(conn, &id);
    }
    let id = unique_id("asset-root", trimmed);
    conn.execute(
        "INSERT INTO asset_roots (id, path, created_at)
         VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        rusqlite::params![id, trimmed],
    )?;
    get_root(conn, &id)
}

pub fn remove_root(conn: &Connection, id: &str) -> CoreResult<bool> {
    conn.execute(
        "DELETE FROM skill_dependencies WHERE skill_id IN (SELECT id FROM skills WHERE root_id = ?1)",
        [id],
    )?;
    conn.execute("DELETE FROM skills WHERE root_id = ?1", [id])?;
    let affected = conn.execute("DELETE FROM asset_roots WHERE id = ?1", [id])?;
    Ok(affected > 0)
}

pub fn mark_root_scanned(conn: &Connection, id: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE asset_roots SET last_scan_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

// ---------- Skill ----------

pub struct SkillUpsert<'a> {
    pub root_id: &'a str,
    pub path: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub category: &'a str,
    pub tags_json: &'a str,
    pub enabled: bool,
    pub source: &'a str,
    pub version: &'a str,
    pub needs_repair: bool,
    pub repair_reason: &'a str,
    pub content_hash: &'a str,
    pub modified_at: Option<&'a str>,
    pub dependencies: &'a [ManifestDependency],
}

/// 写入或更新一个 Skill，返回是否为新增。
pub fn upsert_skill(tx: &Transaction<'_>, input: &SkillUpsert<'_>) -> CoreResult<bool> {
    let mut stmt = tx.prepare("SELECT id FROM skills WHERE root_id = ?1 AND path = ?2")?;
    let existing: Option<String> = stmt
        .query_row(rusqlite::params![input.root_id, input.path], |row| row.get(0))
        .ok();
    drop(stmt);
    let now = now(tx)?;

    let id = match existing {
        Some(id) => {
            tx.execute(
                "UPDATE skills SET
                     name = ?2, description = ?3, category = ?4, tags_json = ?5, enabled = ?6,
                     manifest_source = ?7, version = ?8, needs_repair = ?9, repair_reason = ?10,
                     content_hash = ?11, modified_at = ?12, last_seen_at = ?13, missing = 0
                 WHERE id = ?1",
                rusqlite::params![
                    id,
                    input.name,
                    input.description,
                    input.category,
                    input.tags_json,
                    i64::from(input.enabled),
                    input.source,
                    input.version,
                    i64::from(input.needs_repair),
                    input.repair_reason,
                    input.content_hash,
                    input.modified_at,
                    now
                ],
            )?;
            id
        }
        None => {
            let id = unique_id("skill", input.path);
            tx.execute(
                "INSERT INTO skills
                     (id, root_id, path, name, description, category, tags_json, enabled,
                      manifest_source, version, needs_repair, repair_reason, content_hash,
                      modified_at, first_seen_at, last_seen_at, missing)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, 0)",
                rusqlite::params![
                    id,
                    input.root_id,
                    input.path,
                    input.name,
                    input.description,
                    input.category,
                    input.tags_json,
                    i64::from(input.enabled),
                    input.source,
                    input.version,
                    i64::from(input.needs_repair),
                    input.repair_reason,
                    input.content_hash,
                    input.modified_at,
                    now
                ],
            )?;
            replace_dependencies(tx, &id, input.dependencies)?;
            return Ok(true);
        }
    };

    // 清单可能改过，依赖按最新内容重建。
    replace_dependencies(tx, &id, input.dependencies)?;
    Ok(false)
}

/// 用一组依赖替换 Skill 的既有依赖。
pub fn replace_dependencies(
    tx: &Transaction<'_>,
    skill_id: &str,
    dependencies: &[ManifestDependency],
) -> CoreResult<()> {
    tx.execute("DELETE FROM skill_dependencies WHERE skill_id = ?1", [skill_id])?;
    for dependency in dependencies {
        tx.execute(
            "INSERT INTO skill_dependencies (id, skill_id, name, version, kind)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                unique_id("skill-dep", &format!("{skill_id}{}", dependency.name)),
                skill_id,
                dependency.name,
                dependency.version,
                ""
            ],
        )?;
    }
    Ok(())
}

/// 删除本次扫描未出现的 Skill，返回删除数量。仅根目录可用时调用。
pub fn remove_missing_skills(
    tx: &Transaction<'_>,
    root_id: &str,
    keep: &HashSet<String>,
) -> CoreResult<i64> {
    let mut stmt = tx.prepare("SELECT id, path FROM skills WHERE root_id = ?1")?;
    let rows = stmt.query_map([root_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut stale: Vec<String> = Vec::new();
    for row in rows {
        let (id, path) = row?;
        if !keep.contains(&path) {
            stale.push(id);
        }
    }
    drop(stmt);
    for id in &stale {
        tx.execute("DELETE FROM skill_dependencies WHERE skill_id = ?1", [id])?;
        tx.execute("DELETE FROM skills WHERE id = ?1", [id])?;
    }
    Ok(stale.len() as i64)
}

/// 根目录离线时把它的 Skill 标记为不可读，但保留记录。
pub fn mark_root_skills_missing(conn: &Connection, root_id: &str, missing: bool) -> CoreResult<()> {
    conn.execute(
        "UPDATE skills SET missing = ?2 WHERE root_id = ?1",
        rusqlite::params![root_id, i64::from(missing)],
    )?;
    Ok(())
}

const SKILL_COLUMNS: &str = "s.id, s.root_id, s.name, s.description, s.category, s.tags_json,
    s.enabled, s.manifest_source, s.path, s.version, s.needs_repair, s.repair_reason, s.missing,
    s.modified_at";

fn read_skill(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillView> {
    let tags_json: String = row.get(5)?;
    Ok(SkillView {
        id: row.get(0)?,
        root_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        category: row.get(4)?,
        tags: tags_from_json_text(&tags_json),
        enabled: row.get::<_, i64>(6)? != 0,
        source: row.get(7)?,
        path: row.get(8)?,
        version: row.get(9)?,
        needs_repair: row.get::<_, i64>(10)? != 0,
        repair_reason: row.get(11)?,
        missing: row.get::<_, i64>(12)? != 0,
        modified_at: row.get(13)?,
    })
}

pub fn list_skills(conn: &Connection, filter: &SkillFilter) -> CoreResult<Vec<SkillView>> {
    let limit = filter.limit.unwrap_or(DEFAULT_SKILL_LIMIT).clamp(1, MAX_SKILL_LIMIT);
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(root_id) = filter.root_id.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("s.root_id = ?{}", params.len() + 1));
        params.push(Box::new(root_id.to_string()));
    }
    if let Some(category) = filter.category.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("s.category = ?{}", params.len() + 1));
        params.push(Box::new(category.to_string()));
    }
    if let Some(enabled) = filter.enabled {
        conditions.push(format!("s.enabled = ?{}", params.len() + 1));
        params.push(Box::new(i64::from(enabled)));
    }
    if let Some(needs_repair) = filter.needs_repair {
        conditions.push(format!("s.needs_repair = ?{}", params.len() + 1));
        params.push(Box::new(i64::from(needs_repair)));
    }
    if let Some(tag) = filter.tag.as_deref().filter(|value| !value.is_empty()) {
        conditions.push(format!("s.tags_json LIKE ?{}", params.len() + 1));
        params.push(Box::new(format!("%\"{tag}\"%")));
    }
    if let Some(query) = filter.query.as_deref().filter(|value| !value.is_empty()) {
        let pattern = format!("%{query}%");
        let index = params.len() + 1;
        conditions.push(format!(
            "(s.name LIKE ?{index} OR s.description LIKE ?{index} OR s.path LIKE ?{index})"
        ));
        params.push(Box::new(pattern));
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT {SKILL_COLUMNS} FROM skills s {where_clause}
         ORDER BY s.name ASC, s.path ASC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params.iter().map(|param| param.as_ref())))?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(read_skill(row)?);
    }
    Ok(items)
}

pub fn get_skill(conn: &Connection, id: &str) -> CoreResult<SkillView> {
    let sql = format!("SELECT {SKILL_COLUMNS} FROM skills s WHERE s.id = ?1");
    conn.query_row(&sql, [id], read_skill).map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("Skill 不存在：{id}")),
        other => other.into(),
    })
}

pub fn dependencies(conn: &Connection, skill_id: &str) -> CoreResult<Vec<SkillDependencyView>> {
    let mut stmt = conn.prepare(
        "SELECT name, version, kind FROM skill_dependencies
         WHERE skill_id = ?1 ORDER BY name ASC",
    )?;
    let rows = stmt.query_map([skill_id], |row| {
        Ok(SkillDependencyView {
            name: row.get(0)?,
            version: row.get(1)?,
            kind: row.get(2)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

// ---------- 统计 ----------

pub fn skill_count(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))?;
    Ok(count)
}

pub fn enabled_count(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM skills WHERE enabled = 1", [], |row| {
        row.get(0)
    })?;
    Ok(count)
}

pub fn needs_repair_count(conn: &Connection) -> CoreResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM skills WHERE needs_repair = 1", [], |row| {
        row.get(0)
    })?;
    Ok(count)
}

/// 按分类聚合 Skill 数量与启用数量，空分类归入「未归类」。
pub fn category_stats(conn: &Connection) -> CoreResult<Vec<CategoryStat>> {
    let mut stmt = conn.prepare(
        "SELECT category, COUNT(*), SUM(enabled) FROM skills
         GROUP BY category ORDER BY COUNT(*) DESC, category ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let category: String = row.get(0)?;
        Ok(CategoryStat {
            category: category_label(&category),
            skill_count: row.get(1)?,
            enabled_count: row.get(2)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

/// 近 30 天由扫描累计的新增与移除数量。
pub fn recent_changes(conn: &Connection) -> CoreResult<(i64, i64)> {
    let cutoff = format!("-{RECENT_WINDOW_DAYS} days");
    let (added, removed) = conn.query_row(
        "SELECT COALESCE(SUM(added), 0), COALESCE(SUM(removed), 0) FROM asset_scans
         WHERE created_at >= strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)",
        [cutoff],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((added, removed))
}

/// 已接入的模型平台与状态。
pub fn platform_stats(conn: &Connection) -> CoreResult<Vec<PlatformStat>> {
    let mut stmt = conn.prepare(
        "SELECT code, display_name, model_name, enabled, status FROM ai_platforms
         ORDER BY enabled DESC, display_name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PlatformStat {
            code: row.get(0)?,
            display_name: row.get(1)?,
            model_name: row.get(2)?,
            enabled: row.get::<_, i64>(3)? != 0,
            status: row.get(4)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

/// 记录一次扫描，供资产总览的「近 30 天变动」汇总。
pub fn record_scan(conn: &Connection, outcome: &super::AssetScanOutcome) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO asset_scans
             (id, root_id, root_path, status, scanned, added, updated, removed, needs_repair,
              reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            unique_id("asset-scan", &outcome.root_id),
            outcome.root_id,
            outcome.root_path,
            if outcome.available { "ok" } else { "unavailable" },
            outcome.scanned,
            outcome.added,
            outcome.updated,
            outcome.removed,
            outcome.needs_repair,
            outcome.reason.clone().unwrap_or_default(),
            now(conn)?
        ],
    )?;
    Ok(())
}
