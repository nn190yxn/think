//! 大师仓储：安装、版本化、回退、覆盖矩阵。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::corpus::repo as corpus_repo;
use crate::error::{CoreError, CoreResult};
use crate::master::pack::{self, ValidatedEvidence, ValidatedPack};
use crate::master::{
    CoverageMatrix, DomainCoverage, InstallOutcome, Layer, LayerCoverage, MasterDetail,
    MasterSummary, MasterUnitView, VersionDiff, VersionView, LAYER_ORDER,
};

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

fn layers_to_json(layers: &[Layer]) -> String {
    let names: Vec<&str> = layers.iter().map(|layer| layer.as_str()).collect();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
}

fn layers_from_json(raw: &str) -> Vec<Layer> {
    let names: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    let mut layers: Vec<Layer> = names.iter().filter_map(|name| Layer::parse(name)).collect();
    layers.sort();
    layers
}

/// 承载上一版本技能单元及其引用，供增量更新时原样保留。
struct CarriedUnit {
    title: String,
    layer: Layer,
    trigger_condition: String,
    steps: Vec<String>,
    mechanism: String,
    boundary: String,
    citations: Vec<(Option<String>, String, String)>,
}

enum EvidencePlan {
    /// 来自大师包，需要把 corpusRef 解析成语料记录。
    Pack(Vec<ValidatedEvidence>),
    /// 从上一版本原样带入，直接沿用原有语料记录。
    Carried(Vec<(Option<String>, String, String)>),
}

struct UnitPlan {
    title: String,
    layer: Layer,
    trigger_condition: String,
    steps: Vec<String>,
    mechanism: String,
    boundary: String,
    evidence: EvidencePlan,
}

/// 计算稳定短标识，便于生成可预期的主键。
pub fn short_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    digest.chars().take(16).collect()
}

/// 供测试与调试：确认某位大师是否存在。
pub fn exists(conn: &Connection, master_id: &str) -> CoreResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM masters WHERE id = ?1",
        [master_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// 大师包校验的预检入口：只校验不落库，供安装前预览。
pub fn validate_only(pack_dir: &Path) -> CoreResult<serde_json::Value> {
    let pack = pack::load_and_validate(pack_dir)?;
    Ok(json!({
        "id": pack.id,
        "name": pack.name,
        "domain": pack.domain,
        "version": pack.version,
        "layers": pack.layers.iter().map(|layer| layer.as_str()).collect::<Vec<_>>(),
        "unitCount": pack.units.len(),
        "corpusCount": pack.corpus.len(),
    }))
}

/// 安装或更新一位大师。首次安装要求 version 为 1；
/// 更新要求版本号递增，并在既有框架之上增量补充。
pub fn install(conn: &mut Connection, pack_dir: &Path) -> CoreResult<InstallOutcome> {
    let validated = pack::load_and_validate(pack_dir)?;
    let install_time = now(conn)?;
    let tx = conn.transaction()?;

    let existing: Option<i64> = tx
        .query_row(
            "SELECT current_version FROM masters WHERE id = ?1",
            [&validated.id],
            |row| row.get(0),
        )
        .optional()?;

    let (current_version, created) = match existing {
        None => {
            if validated.version != 1 {
                return Err(CoreError::InvalidInput(format!(
                    "首次安装的版本号必须为 1，实际为 {}",
                    validated.version
                )));
            }
            (0i64, true)
        }
        Some(version) => {
            if validated.version <= version {
                return Err(CoreError::InvalidInput(format!(
                    "版本号必须递增：当前为 v{version}，大师包为 v{}",
                    validated.version
                )));
            }
            (version, false)
        }
    };

    let carried = if created {
        Vec::new()
    } else {
        load_carried_units(&tx, &validated.id, current_version)?
    };

    let (mut plans, diff) = merge_units(carried, &validated);
    let corpus_map = register_corpus(&tx, &validated, &install_time)?;

    tx.execute(
        "INSERT INTO masters (id, name, domain, layers_json, summary, style, blind_spots, status,
                              current_version, installed_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ready', ?8, ?9, ?9)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             domain = excluded.domain,
             layers_json = excluded.layers_json,
             summary = excluded.summary,
             style = excluded.style,
             blind_spots = excluded.blind_spots,
             current_version = excluded.current_version,
             updated_at = excluded.updated_at",
        rusqlite::params![
            validated.id,
            validated.name,
            validated.domain,
            layers_to_json(&validated.layers),
            validated.summary,
            validated.style,
            validated.blind_spots,
            validated.version,
            install_time,
        ],
    )?;

    tx.execute(
        "INSERT INTO master_versions (master_id, version, unit_count, source_refs_json, diff_json,
                                      note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            validated.id,
            validated.version,
            plans.len() as i64,
            serde_json::to_string(&diff.source_refs).unwrap_or_else(|_| "[]".to_string()),
            serde_json::to_string(&diff).unwrap_or_else(|_| "{}".to_string()),
            validated.note,
            install_time,
        ],
    )?;

    // 同一版本重复写入时先清空，保证安装幂等。
    tx.execute(
        "DELETE FROM master_units WHERE master_id = ?1 AND version = ?2",
        rusqlite::params![validated.id, validated.version],
    )?;

    let unit_total = plans.len() as i64;
    for (ordinal, plan) in plans.drain(..).enumerate() {
        let unit_id = format!("{}:v{}:{:03}", validated.id, validated.version, ordinal + 1);
        tx.execute(
            "INSERT INTO master_units (id, master_id, version, ordinal, title, layer,
                                       trigger_condition, steps_json, mechanism, boundary, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                unit_id,
                validated.id,
                validated.version,
                ordinal as i64,
                plan.title,
                plan.layer.as_str(),
                plan.trigger_condition,
                serde_json::to_string(&plan.steps).unwrap_or_else(|_| "[]".to_string()),
                plan.mechanism,
                plan.boundary,
                install_time,
            ],
        )?;

        let citations: Vec<(Option<String>, String, String)> = match plan.evidence {
            EvidencePlan::Pack(items) => items
                .into_iter()
                .map(|item| {
                    (
                        corpus_map.get(&item.corpus_ref).cloned(),
                        item.excerpt,
                        item.location,
                    )
                })
                .collect(),
            EvidencePlan::Carried(items) => items,
        };

        for (index, (corpus_id, excerpt, location)) in citations.into_iter().enumerate() {
            tx.execute(
                "INSERT INTO corpus_citations (id, master_unit_id, corpus_item_id, excerpt,
                                               location, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    format!("{unit_id}:c{}", index + 1),
                    unit_id,
                    corpus_id,
                    excerpt,
                    location,
                    install_time,
                ],
            )?;
        }
    }

    let corpus_count = validated.corpus.len() as i64;
    let master_id = validated.id.clone();
    let version = validated.version;
    tx.commit()?;

    Ok(InstallOutcome {
        master_id,
        version,
        unit_count: unit_total,
        corpus_count,
        created,
        diff,
    })
}

/// 合并既有单元与新包单元：同标题以新包为准，其余保留原位置。
fn merge_units(carried: Vec<CarriedUnit>, pack: &ValidatedPack) -> (Vec<UnitPlan>, VersionDiff) {
    let carried_count = carried.len();
    let mut plans: Vec<UnitPlan> = carried
        .into_iter()
        .map(|unit| UnitPlan {
            title: unit.title,
            layer: unit.layer,
            trigger_condition: unit.trigger_condition,
            steps: unit.steps,
            mechanism: unit.mechanism,
            boundary: unit.boundary,
            evidence: EvidencePlan::Carried(unit.citations),
        })
        .collect();

    let mut added = Vec::new();
    let mut updated = Vec::new();

    for unit in &pack.units {
        let plan = UnitPlan {
            title: unit.title.clone(),
            layer: unit.layer,
            trigger_condition: unit.trigger_condition.clone(),
            steps: unit.steps.clone(),
            mechanism: unit.mechanism.clone(),
            boundary: unit.boundary.clone(),
            evidence: EvidencePlan::Pack(unit.evidence.clone()),
        };
        match plans.iter().position(|existing| existing.title == unit.title) {
            Some(position) => {
                // 内容完全一致时视为保留，只有真正改动的单元才记为更新。
                if same_content(&plans[position], &plan) {
                    continue;
                }
                plans[position] = plan;
                updated.push(unit.title.clone());
            }
            None => {
                plans.push(plan);
                added.push(unit.title.clone());
            }
        }
    }

    let diff = VersionDiff {
        added,
        carried: carried_count - updated.len(),
        source_refs: pack
            .corpus
            .iter()
            .map(|item| item.reference.clone())
            .collect(),
        updated,
    };

    (plans, diff)
}

/// 比较四要素与步骤是否一致，用于区分「重声明」与「真更新」。
fn same_content(left: &UnitPlan, right: &UnitPlan) -> bool {
    left.layer == right.layer
        && left.trigger_condition == right.trigger_condition
        && left.steps == right.steps
        && left.mechanism == right.mechanism
        && left.boundary == right.boundary
}

fn load_carried_units(
    tx: &Transaction<'_>,
    master_id: &str,
    version: i64,
) -> CoreResult<Vec<CarriedUnit>> {
    let mut stmt = tx.prepare(
        "SELECT id, title, layer, trigger_condition, steps_json, mechanism, boundary
         FROM master_units
         WHERE master_id = ?1 AND version = ?2
         ORDER BY ordinal ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![master_id, version], |row| {
        let steps_json: String = row.get(4)?;
        let layer: String = row.get(2)?;
        Ok((
            row.get::<_, String>(0)?,
            CarriedUnit {
                title: row.get(1)?,
                layer: Layer::parse(&layer).unwrap_or(Layer::Fa),
                trigger_condition: row.get(3)?,
                steps: serde_json::from_str(&steps_json).unwrap_or_default(),
                mechanism: row.get(5)?,
                boundary: row.get(6)?,
                citations: Vec::new(),
            },
        ))
    })?;

    let mut units = Vec::new();
    for row in rows {
        let (unit_id, mut unit) = row?;
        unit.citations = load_citations(tx, &unit_id)?;
        units.push(unit);
    }
    Ok(units)
}

fn load_citations(
    tx: &Transaction<'_>,
    unit_id: &str,
) -> CoreResult<Vec<(Option<String>, String, String)>> {
    let mut stmt = tx.prepare(
        "SELECT corpus_item_id, excerpt, location FROM corpus_citations
         WHERE master_unit_id = ?1
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([unit_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    let mut citations = Vec::new();
    for row in rows {
        citations.push(row?);
    }
    Ok(citations)
}

/// 注册语料并返回 ref 到语料记录 id 的映射。
fn register_corpus(
    tx: &Transaction<'_>,
    pack: &ValidatedPack,
    registered_at: &str,
) -> CoreResult<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for item in &pack.corpus {
        let source_ref = item.resolved_path.to_string_lossy().to_string();
        let normalized = corpus_repo::normalize_name(&item.title);

        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM corpus_items WHERE source_kind = ?1 AND source_ref = ?2",
                rusqlite::params![item.kind, source_ref],
                |row| row.get(0),
            )
            .optional()?;

        let id = match existing {
            Some(id) => {
                tx.execute(
                    "UPDATE corpus_items SET title = ?1, normalized_name = ?2, content_hash = ?3,
                                             byte_size = ?4, location_hint = ?5, available = 1
                     WHERE id = ?6",
                    rusqlite::params![
                        item.title,
                        normalized,
                        item.content_hash,
                        item.byte_size as i64,
                        item.location_hint,
                        id,
                    ],
                )?;
                id
            }
            None => {
                let id = corpus_repo::deterministic_id(&item.kind, &source_ref);
                tx.execute(
                    "INSERT INTO corpus_items (id, source_kind, source_ref, title, normalized_name,
                                               master_ids_json, location_hint, content_hash,
                                               byte_size, available, registered_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, '[]', ?6, ?7, ?8, 1, ?9)",
                    rusqlite::params![
                        id,
                        item.kind,
                        source_ref,
                        item.title,
                        normalized,
                        item.location_hint,
                        item.content_hash,
                        item.byte_size as i64,
                        registered_at,
                    ],
                )?;
                id
            }
        };

        corpus_repo::attach_master(tx, &id, &pack.id)?;
        corpus_repo::sync_search_index(tx, &id, &item.title, &normalized, &source_ref)?;
        map.insert(item.reference.clone(), id);
    }
    Ok(map)
}

pub fn list(
    conn: &Connection,
    domain: Option<&str>,
    layer: Option<Layer>,
) -> CoreResult<Vec<MasterSummary>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.domain, s.layers_json, s.status, s.current_version,
                s.installed_at, s.updated_at,
                (SELECT COUNT(*) FROM master_units u
                  WHERE u.master_id = s.id AND u.version = s.current_version) AS unit_count
         FROM masters s
         ORDER BY s.domain ASC, s.name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let layers_json: String = row.get(3)?;
        Ok(MasterSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            domain: row.get(2)?,
            layers: layers_from_json(&layers_json),
            status: row.get(4)?,
            current_version: row.get(5)?,
            unit_count: row.get(8)?,
            installed_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut masters = Vec::new();
    for row in rows {
        let master = row?;
        if let Some(domain) = domain {
            if master.domain != domain {
                continue;
            }
        }
        if let Some(layer) = layer {
            if !master.layers.contains(&layer) {
                continue;
            }
        }
        masters.push(master);
    }
    Ok(masters)
}

pub fn detail(conn: &Connection, master_id: &str) -> CoreResult<MasterDetail> {
    let row = conn
        .query_row(
            "SELECT id, name, domain, layers_json, summary, style, blind_spots, status,
                    current_version
             FROM masters WHERE id = ?1",
            [master_id],
            |row| {
                let layers_json: String = row.get(3)?;
                Ok(MasterDetail {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    domain: row.get(2)?,
                    layers: layers_from_json(&layers_json),
                    summary: row.get(4)?,
                    style: row.get(5)?,
                    blind_spots: row.get(6)?,
                    status: row.get(7)?,
                    current_version: row.get(8)?,
                    units: Vec::new(),
                    versions: Vec::new(),
                })
            },
        )
        .optional()?;

    let mut detail = row.ok_or_else(|| CoreError::NotFound(format!("大师 {master_id}")))?;
    let availability = corpus_repo::availability_map(conn)?;

    let mut stmt = conn.prepare(
        "SELECT id, title, layer, trigger_condition, steps_json, mechanism, boundary, flagged_reason
         FROM master_units
         WHERE master_id = ?1 AND version = ?2
         ORDER BY ordinal ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![master_id, detail.current_version], |row| {
        let steps_json: String = row.get(4)?;
        let layer: String = row.get(2)?;
        Ok(MasterUnitView {
            id: row.get(0)?,
            title: row.get(1)?,
            layer: Layer::parse(&layer).unwrap_or(Layer::Fa),
            trigger_condition: row.get(3)?,
            steps: serde_json::from_str(&steps_json).unwrap_or_default(),
            mechanism: row.get(5)?,
            boundary: row.get(6)?,
            flagged_reason: row.get(7)?,
            citations: Vec::new(),
        })
    })?;

    for row in rows {
        let mut unit = row?;
        unit.citations = corpus_repo::citations_of_unit(conn, &unit.id, &availability)?;
        detail.units.push(unit);
    }

    detail.versions = versions(conn, master_id)?;
    Ok(detail)
}

pub fn versions(conn: &Connection, master_id: &str) -> CoreResult<Vec<VersionView>> {
    let mut stmt = conn.prepare(
        "SELECT version, unit_count, note, created_at, diff_json
         FROM master_versions WHERE master_id = ?1
         ORDER BY version DESC",
    )?;
    let rows = stmt.query_map([master_id], |row| {
        let diff_json: String = row.get(4)?;
        Ok(VersionView {
            version: row.get(0)?,
            unit_count: row.get(1)?,
            note: row.get(2)?,
            created_at: row.get(3)?,
            diff: serde_json::from_str(&diff_json).unwrap_or_default(),
        })
    })?;
    let mut versions = Vec::new();
    for row in rows {
        versions.push(row?);
    }
    Ok(versions)
}

/// 回退只移动当前版本指针，更高版本的快照原样保留。
pub fn revert(conn: &Connection, master_id: &str, version: i64) -> CoreResult<i64> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM master_versions WHERE master_id = ?1 AND version = ?2",
        rusqlite::params![master_id, version],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(CoreError::NotFound(format!(
            "大师 {master_id} 的 v{version}"
        )));
    }

    let updated_at = now(conn)?;
    conn.execute(
        "UPDATE masters SET current_version = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![master_id, version, updated_at],
    )?;
    Ok(version)
}

pub fn flag_unit(conn: &Connection, unit_id: &str, reason: &str) -> CoreResult<()> {
    if reason.trim().is_empty() {
        return Err(CoreError::InvalidInput("反馈原因不能为空".into()));
    }
    let affected = conn.execute(
        "UPDATE master_units SET flagged_reason = ?2 WHERE id = ?1",
        rusqlite::params![unit_id, reason.trim()],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("技能单元 {unit_id}")));
    }
    Ok(())
}

/// 覆盖矩阵：层次与领域的交叉统计，并给出层次空缺建议。
pub fn coverage_matrix(conn: &Connection) -> CoreResult<CoverageMatrix> {
    let masters = list(conn, None, None)?;

    let mut layer_masters: BTreeMap<Layer, Vec<String>> = BTreeMap::new();
    for layer in LAYER_ORDER {
        layer_masters.insert(layer, Vec::new());
    }
    for master in &masters {
        for layer in &master.layers {
            if let Some(entry) = layer_masters.get_mut(layer) {
                entry.push(master.name.clone());
            }
        }
    }

    let mut unit_counts: BTreeMap<Layer, i64> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT u.layer, COUNT(*)
         FROM master_units u
         JOIN masters s ON s.id = u.master_id AND s.current_version = u.version
         GROUP BY u.layer",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (layer, count) = row?;
        if let Some(layer) = Layer::parse(&layer) {
            unit_counts.insert(layer, count);
        }
    }

    let layers: Vec<LayerCoverage> = LAYER_ORDER
        .iter()
        .map(|layer| {
            let names = layer_masters.get(layer).cloned().unwrap_or_default();
            LayerCoverage {
                layer: *layer,
                name: layer.name().to_string(),
                master_count: names.len() as i64,
                unit_count: *unit_counts.get(layer).unwrap_or(&0),
                masters: names,
            }
        })
        .collect();

    let mut by_domain: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for master in &masters {
        by_domain
            .entry(master.domain.clone())
            .or_default()
            .push(master.id.clone());
    }

    let mut domains = Vec::new();
    for (domain, ids) in &by_domain {
        let mut present: Vec<Layer> = Vec::new();
        for id in ids {
            if let Some(master) = masters.iter().find(|master| &master.id == id) {
                for layer in &master.layers {
                    if !present.contains(layer) {
                        present.push(*layer);
                    }
                }
            }
        }
        present.sort();
        let missing: Vec<Layer> = LAYER_ORDER
            .iter()
            .copied()
            .filter(|layer| !present.contains(layer))
            .collect();
        domains.push(DomainCoverage {
            domain: domain.clone(),
            master_count: ids.len() as i64,
            present_layers: present,
            missing_layers: missing,
        });
    }

    let suggestions: Vec<String> = layers
        .iter()
        .filter(|entry| entry.master_count == 0)
        .map(|entry| {
            format!(
                "{}层尚无大师，建议至少补充一位，否则六席选角无法覆盖该层",
                entry.name
            )
        })
        .collect();

    Ok(CoverageMatrix {
        master_count: masters.len() as i64,
        layers,
        domains,
        suggestions,
    })
}

/// 供前端展示的领域清单。
pub fn domains(conn: &Connection) -> CoreResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT domain FROM masters ORDER BY domain ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut domains = Vec::new();
    for row in rows {
        domains.push(row?);
    }
    Ok(domains)
}
