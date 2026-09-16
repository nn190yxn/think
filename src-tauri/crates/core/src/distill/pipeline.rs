//! 六阶段蒸馏流水线：整体理解、五路提取、三重验证、技能单元、技能地图、
//! 压力测试与交付安装。每个阶段结束写入检查点，中断后按 `stage` 指针续跑。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Deserialize;

use crate::error::{CoreError, CoreResult};
use crate::llm::{call_model, ModelClient, ModelRequest, RetryPolicy};
use crate::master::{self, repo as master_repo, Layer, LAYER_ORDER};

use super::repo;
use super::{
    CandidateDraft, DistillDraft, DistillJobView, DistillStage, DistillState, ExcludedCandidate,
    ExtractTrack, IntakeMaterial, SkillGroup, SkillLink, SkillUnitDraft, SkeletonDraft,
    StressCaseDraft, DUPLICATE_SIMILARITY_THRESHOLD, EXTRACT_TRACKS, MAX_MATERIAL_CHARS,
    PURPOSE_COMPOSE, PURPOSE_EXTRACT, PURPOSE_SKELETON, PURPOSE_STRESS, PURPOSE_VERIFY,
    STRESS_PASS_THRESHOLD,
};

/// 单次材料上下文的字符上限，避免把整本书一次送进模型。
const MAX_CONTEXT_CHARS: usize = 12_000;
/// 技能地图交叉链接数量上限。
const MAX_SKILL_LINKS: usize = 200;

/// 新建蒸馏任务的入参。
pub struct DistillInput {
    pub master_id: String,
    pub master_name: String,
    pub domain: String,
    pub source_kind: String,
    pub source_ref: String,
    pub materials: Vec<IntakeMaterial>,
    pub output_dir: PathBuf,
    /// 上次蒸馏被标记为不适用的判断，作为负面依据注入提示词。
    pub negative: Vec<String>,
}

type StageResult = (DistillStage, DistillState, DistillDraft, i64);

/// 新建任务并推进到第一个门。骨架确认门会停在阶段0之后。
pub fn start(
    conn: &mut Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    input: &DistillInput,
) -> CoreResult<DistillJobView> {
    if input.materials.is_empty() {
        return Err(CoreError::InvalidInput(
            "蒸馏至少需要一份带正文或文件的材料".to_string(),
        ));
    }
    if input.materials.len() > super::MAX_MATERIALS {
        return Err(CoreError::InvalidInput(format!(
            "材料数量超过上限 {}",
            super::MAX_MATERIALS
        )));
    }
    for material in &input.materials {
        if material.title.trim().is_empty() {
            return Err(CoreError::InvalidInput("材料标题不能为空".to_string()));
        }
        let has_file = !material.source_ref.trim().is_empty() && Path::new(&material.source_ref).is_file();
        if material.text.trim().is_empty() && !has_file {
            return Err(CoreError::InvalidInput(format!(
                "材料「{}」既没有正文，也没有可读文件",
                material.title
            )));
        }
    }

    let job = repo::create_job(
        conn,
        &repo::NewDistillJob {
            source_kind: &input.source_kind,
            source_ref: &input.source_ref,
            master_id: &input.master_id,
            master_name: &input.master_name,
            domain: &input.domain,
            output_dir: &input.output_dir.to_string_lossy(),
            materials: &input.materials,
            negative: &input.negative,
        },
    )?;
    advance(conn, client, policy, &job.id)
}

/// 用户确认骨架后继续；只有处于确认门时才推进。
pub fn confirm_skeleton(
    conn: &mut Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job_id: &str,
) -> CoreResult<DistillJobView> {
    let job = repo::get_job(conn, job_id)?;
    match job.state {
        DistillState::AwaitingConfirmation => {
            repo::set_state(conn, job_id, DistillState::Running, None)?;
            advance(conn, client, policy, job_id)
        }
        _ => Ok(job),
    }
}

/// 失败后从检查点续跑；已完成阶段不重复执行。
pub fn resume(
    conn: &mut Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job_id: &str,
) -> CoreResult<DistillJobView> {
    let job = repo::get_job(conn, job_id)?;
    match job.state {
        DistillState::Failed | DistillState::Pending => {
            repo::set_state(conn, job_id, DistillState::Running, None)?;
            advance(conn, client, policy, job_id)
        }
        _ => Ok(job),
    }
}

/// 从当前阶段连续推进，直到门、完成或失败。
pub fn advance(
    conn: &mut Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job_id: &str,
) -> CoreResult<DistillJobView> {
    let mut job = repo::get_job(conn, job_id)?;
    loop {
        match job.state {
            DistillState::AwaitingConfirmation | DistillState::Done | DistillState::Failed => {
                return Ok(job);
            }
            DistillState::Pending | DistillState::Running => {}
        }
        if job.stage == DistillStage::Done {
            repo::set_state(conn, job_id, DistillState::Done, None)?;
            return repo::get_job(conn, job_id);
        }

        let stage = job.stage;
        match run_stage(conn, client, policy, &job, stage) {
            Ok((next, state, draft, calls)) => {
                repo::save_progress(conn, job_id, next, state, &draft, None, calls)?;
                job = repo::get_job(conn, job_id)?;
                if state != DistillState::Running {
                    return Ok(job);
                }
            }
            Err(error) => {
                repo::set_state(conn, job_id, DistillState::Failed, Some(error.code()))?;
                return repo::get_job(conn, job_id);
            }
        }
    }
}

fn run_stage(
    conn: &mut Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job: &DistillJobView,
    stage: DistillStage,
) -> CoreResult<StageResult> {
    let draft = repo::get_draft(conn, &job.id)?;
    match stage {
        DistillStage::Skeleton => run_skeleton(conn, client, policy, job, draft),
        DistillStage::Extract => run_extract(conn, client, policy, job, draft),
        DistillStage::Verify => run_verify(conn, client, policy, job, draft),
        DistillStage::Compose => run_compose(conn, client, policy, job, draft),
        DistillStage::Map => Ok(run_map(draft)),
        DistillStage::Stress => run_stress(conn, client, policy, job, draft),
        DistillStage::Deliver => run_deliver(conn, job, draft),
        DistillStage::Done => Ok((DistillStage::Done, DistillState::Done, draft, 0)),
    }
}

// ---------- 文本工具 ----------

/// 取字符串中最外层的 JSON 数组，容忍代码块围栏与夹带说明。
fn json_array(raw: &str) -> &str {
    match (raw.find('['), raw.rfind(']')) {
        (Some(start), Some(end)) if end > start => &raw[start..=end],
        _ => "[]",
    }
}

/// 取字符串中最外层的 JSON 对象。
fn json_object(raw: &str) -> &str {
    match (raw.find('{'), raw.rfind('}')) {
        (Some(start), Some(end)) if end > start => &raw[start..=end],
        _ => "{}",
    }
}

fn truncated(value: &str, limit: usize) -> String {
    let chars: Vec<char> = value.trim().chars().collect();
    if chars.len() <= limit {
        return chars.into_iter().collect();
    }
    let mut text: String = chars[..limit].iter().collect();
    text.push('…');
    text
}

fn bigrams(text: &str) -> BTreeSet<String> {
    let chars: Vec<char> = text
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect();
    if chars.len() < 2 {
        return chars.iter().map(|c| c.to_string()).collect();
    }
    chars
        .windows(2)
        .map(|pair| pair.iter().collect::<String>())
        .collect()
}

/// 字符二元组 Jaccard 相似度，用于重复判定与重叠度评估。
pub fn similarity(left: &str, right: &str) -> f64 {
    let a = bigrams(left);
    let b = bigrams(right);
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(&b).count() as f64;
    let union = a.union(&b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// 材料上下文的公共构造，限制总字符数并标注来源。
fn materials_context(materials: &[IntakeMaterial]) -> String {
    let mut out = String::new();
    for (index, material) in materials.iter().enumerate() {
        if out.chars().count() >= MAX_CONTEXT_CHARS {
            out.push_str("\n（材料过多，其余从略）\n");
            break;
        }
        out.push_str(&format!(
            "\n【材料{}】{}（{}）\n来源：{}\n",
            index + 1,
            material.title,
            material.kind,
            if material.source_ref.trim().is_empty() {
                "用户输入"
            } else {
                material.source_ref.trim()
            }
        ));
        let body = if material.text.trim().is_empty() {
            "（仅登记文件，正文未读取）"
        } else {
            material.text.trim()
        };
        out.push_str(&truncated(body, MAX_MATERIAL_CHARS));
        out.push('\n');
    }
    out
}

fn negative_context(negative: &[String]) -> String {
    if negative.is_empty() {
        String::new()
    } else {
        let mut out = String::from("\n以下判断此前被用户标记为不适用，请避免重复：\n");
        for item in negative {
            out.push_str(&format!("- {item}\n"));
        }
        out
    }
}

fn call_stage(
    conn: &Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    purpose: &str,
    system: &str,
    user: &str,
) -> CoreResult<String> {
    let mut request = ModelRequest::new(purpose, system, user);
    request.max_tokens = 2000;
    let response = call_model(conn, client, &request, policy)?;
    Ok(response.content)
}

// ---------- 阶段0 整体理解 ----------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawSkeleton {
    summary: String,
    domain: String,
    layers: Vec<String>,
    themes: Vec<String>,
    angles: Vec<String>,
}

fn run_skeleton(
    conn: &Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job: &DistillJobView,
    mut draft: DistillDraft,
) -> CoreResult<StageResult> {
    let materials = repo::job_materials(conn, &job.id)?;
    let negative = repo::job_negative(conn, &job.id)?;
    let system = "你在为思想熔炉蒸馏一位大师。先做整体理解：概括他的思想主线、所属领域、\
可覆盖的层次与核心主题，供用户确认骨架。严格只输出 JSON 对象，不要解释或代码块标记。格式：\
{\"summary\":\"三句以内主线概括\",\"domain\":\"领域\",\"layers\":[\"dao|fa|shu|qi|tool|shi\"],\
\"themes\":[\"主题\"],\"angles\":[\"切入角度\"]}。";
    let user = format!(
        "大师：{}（领域：{}）\n材料：{}\n{}请给出整体理解骨架。",
        job.master_name,
        job.domain,
        materials_context(&materials),
        negative_context(&negative),
    );
    let raw = call_stage(conn, client, policy, PURPOSE_SKELETON, system, &user)?;
    let parsed: RawSkeleton = serde_json::from_str(json_object(&raw)).unwrap_or_default();

    let mut layers: Vec<String> = parsed
        .layers
        .iter()
        .filter(|name| Layer::parse(name).is_some())
        .map(|name| name.to_ascii_lowercase())
        .collect();
    layers.dedup();
    if layers.is_empty() {
        layers = LAYER_ORDER.iter().map(|layer| layer.as_str().to_string()).collect();
    }
    let domain = if parsed.domain.trim().is_empty() {
        job.domain.clone()
    } else {
        parsed.domain.trim().to_string()
    };

    draft.skeleton = Some(SkeletonDraft {
        summary: parsed.summary.trim().to_string(),
        domain,
        layers,
        themes: parsed.themes,
        angles: parsed.angles,
    });
    Ok((DistillStage::Extract, DistillState::AwaitingConfirmation, draft, 1))
}

// ---------- 阶段1 五路并行提取 ----------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawCandidate {
    title: String,
    summary: String,
    layer: String,
    evidence: Vec<String>,
}

fn run_extract(
    conn: &Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job: &DistillJobView,
    mut draft: DistillDraft,
) -> CoreResult<StageResult> {
    let materials = repo::job_materials(conn, &job.id)?;
    let negative = repo::job_negative(conn, &job.id)?;
    let skeleton = draft.skeleton.clone().unwrap_or_default();
    let context = materials_context(&materials);
    let negative_text = negative_context(&negative);
    let mut extracted = Vec::new();
    let mut calls = 0i64;

    for track in EXTRACT_TRACKS {
        let purpose = format!("{PURPOSE_EXTRACT}_{}", track.as_str());
        let system = format!(
            "你在从材料中做单一路径的提取：{}。只从材料出发，不要补充材料之外的内容。\
严格只输出 JSON 数组，不要解释或代码块标记。每个元素格式：\
{{\"title\":\"一句话标题\",\"summary\":\"两句以内说明\",\"layer\":\"dao|fa|shu|qi|tool|shi\",\
\"evidence\":[\"材料编号或原文片段\"]}}。没有可提取内容时输出 []。",
            track.instruction()
        );
        let user = format!(
            "大师：{}\n已确认骨架：{}\n主题：{}\n材料：{}\n{}请只做「{}」这一路的提取。",
            job.master_name,
            skeleton.summary,
            skeleton.themes.join("、"),
            context,
            negative_text,
            track.name(),
        );
        let raw = call_stage(conn, client, policy, &purpose, &system, &user)?;
        calls += 1;
        let items: Vec<RawCandidate> =
            serde_json::from_str(json_array(&raw)).unwrap_or_default();
        for (index, item) in items.into_iter().enumerate() {
            let title = item.title.trim().to_string();
            if title.is_empty() {
                continue;
            }
            let layer = item
                .layer
                .trim()
                .to_ascii_lowercase();
            let layer = if Layer::parse(&layer).is_some() {
                layer
            } else {
                default_layer_for(track).to_string()
            };
            extracted.push(CandidateDraft {
                id: format!("cand-{}-{}", track.as_str(), index + 1),
                track: track.as_str().to_string(),
                title,
                summary: item.summary.trim().to_string(),
                layer,
                evidence: item.evidence,
            });
        }
    }

    if extracted.is_empty() {
        return Err(CoreError::MalformedResponse(
            "五路提取未产出任何候选".to_string(),
        ));
    }
    draft.extracted = extracted;
    Ok((DistillStage::Verify, DistillState::Running, draft, calls))
}

fn default_layer_for(track: ExtractTrack) -> &'static str {
    match track {
        ExtractTrack::Framework => "fa",
        ExtractTrack::Principle => "dao",
        ExtractTrack::Case => "shi",
        ExtractTrack::Counterexample => "qi",
        ExtractTrack::Term => "tool",
    }
}

// ---------- 阶段1.5 三重验证 ----------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawVerdict {
    id: String,
    title: String,
    cross_domain: bool,
    answers_new: bool,
    reason: String,
}

fn run_verify(
    conn: &Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job: &DistillJobView,
    mut draft: DistillDraft,
) -> CoreResult<StageResult> {
    let system = "你在做三重验证。对每个候选判断三件事：是否有跨材料的独立佐证、\
是否能回答材料未明说的新问题、与已知方法论是否重复。严格只输出 JSON 数组，\
每个元素格式：{\"id\":\"候选id\",\"crossDomain\":true,\"answersNew\":true,\"reason\":\"未通过时说明原因\"}。";
    let mut listing = String::new();
    for candidate in &draft.extracted {
        listing.push_str(&format!(
            "- [{}] ({}) {}：{}\n",
            candidate.id,
            candidate.track,
            candidate.title,
            candidate.summary
        ));
    }
    let user = format!("请验证以下候选：\n{listing}");
    let raw = call_stage(conn, client, policy, PURPOSE_VERIFY, system, &user)?;
    let verdicts: Vec<RawVerdict> = serde_json::from_str(json_array(&raw)).unwrap_or_default();

    let known_units = known_unit_texts(conn, &job.master_id)?;
    let mut verified = Vec::new();
    let mut excluded = draft.excluded.clone();
    for candidate in &draft.extracted {
        let verdict = verdicts
            .iter()
            .find(|item| item.id == candidate.id || item.title == candidate.title);
        let mut failures = Vec::new();
        match verdict {
            Some(verdict) => {
                if !verdict.cross_domain {
                    failures.push("缺少跨域独立佐证".to_string());
                }
                if !verdict.answers_new {
                    failures.push("无法回答材料未明说的新问题".to_string());
                }
            }
            None => failures.push("未获得验证结论".to_string()),
        }
        let duplicate = known_units.iter().any(|known| {
            similarity(&format!("{} {}", candidate.title, candidate.summary), known)
                >= DUPLICATE_SIMILARITY_THRESHOLD
        });
        if duplicate {
            failures.push("与已有方法论重复".to_string());
        }

        if failures.is_empty() {
            verified.push(candidate.clone());
        } else {
            let reason = match verdict {
                Some(verdict) if !verdict.reason.trim().is_empty() => {
                    format!("{}（{}）", failures.join("、"), verdict.reason.trim())
                }
                _ => failures.join("、"),
            };
            excluded.push(ExcludedCandidate {
                id: candidate.id.clone(),
                track: candidate.track.clone(),
                title: candidate.title.clone(),
                stage: "verify".to_string(),
                reason,
            });
        }
    }

    draft.verified = verified;
    draft.excluded = excluded;
    Ok((DistillStage::Compose, DistillState::Running, draft, 1))
}

/// 已安装大师的技能单元标题与机制，用于重复判定。
fn known_unit_texts(conn: &Connection, master_id: &str) -> CoreResult<Vec<String>> {
    if !master_repo::exists(conn, master_id)? {
        return Ok(Vec::new());
    }
    let detail = master_repo::detail(conn, master_id)?;
    Ok(detail
        .units
        .iter()
        .map(|unit| format!("{} {}", unit.title, unit.mechanism))
        .collect())
}

// ---------- 阶段2 技能单元 ----------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawUnit {
    candidate_id: String,
    title: String,
    layer: String,
    trigger_condition: String,
    steps: Vec<String>,
    mechanism: String,
    boundary: String,
    evidence: Vec<String>,
}

fn run_compose(
    conn: &Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job: &DistillJobView,
    mut draft: DistillDraft,
) -> CoreResult<StageResult> {
    if draft.verified.is_empty() {
        return Err(CoreError::MalformedResponse(
            "没有通过三重验证的候选，无法生成技能单元".to_string(),
        ));
    }
    let system = "你在把候选转成可调用的技能单元。每个单元必须包含四要素：触发条件、执行步骤、\
作用机制、适用边界，并标注它来自哪个候选。只使用通过验证的候选，不得新增候选。\
严格只输出 JSON 数组，每个元素格式：{\"candidateId\":\"候选id\",\"title\":\"单元标题\",\
\"layer\":\"dao|fa|shu|qi|tool|shi\",\"triggerCondition\":\"何时用\",\"steps\":[\"步骤\"],\
\"mechanism\":\"为什么有效\",\"boundary\":\"何时不适用\",\"evidence\":[\"材料编号或原文片段\"]}。";
    let mut listing = String::new();
    for candidate in &draft.verified {
        listing.push_str(&format!(
            "- [{}]（{}，建议层次 {}）{}：{}\n",
            candidate.id, candidate.track, candidate.layer, candidate.title, candidate.summary
        ));
    }
    let user = format!(
        "大师：{}（领域：{}）\n请基于以下候选生成技能单元：\n{listing}",
        job.master_name, job.domain
    );
    let raw = call_stage(conn, client, policy, PURPOSE_COMPOSE, system, &user)?;
    let parsed: Vec<RawUnit> = serde_json::from_str(json_array(&raw)).unwrap_or_default();

    let verified_ids: BTreeSet<&str> =
        draft.verified.iter().map(|candidate| candidate.id.as_str()).collect();
    let mut units = Vec::new();
    let mut excluded = draft.excluded.clone();
    for (index, item) in parsed.into_iter().enumerate() {
        let title = item.title.trim().to_string();
        let display = if title.is_empty() {
            format!("未命名单元{}", index + 1)
        } else {
            title.clone()
        };
        let candidate_id = item.candidate_id.trim().to_string();
        if !verified_ids.contains(candidate_id.as_str()) {
            excluded.push(ExcludedCandidate {
                id: candidate_id,
                track: "compose".to_string(),
                title: display,
                stage: "compose".to_string(),
                reason: "候选未通过三重验证，不进入技能单元".to_string(),
            });
            continue;
        }
        let steps: Vec<String> = item
            .steps
            .iter()
            .map(|step| step.trim().to_string())
            .filter(|step| !step.is_empty())
            .collect();
        let missing = title.is_empty()
            || item.trigger_condition.trim().is_empty()
            || steps.is_empty()
            || item.mechanism.trim().is_empty()
            || item.boundary.trim().is_empty();
        if missing {
            excluded.push(ExcludedCandidate {
                id: candidate_id,
                track: "compose".to_string(),
                title: display,
                stage: "compose".to_string(),
                reason: "四要素不全：需要触发条件、执行步骤、作用机制与适用边界".to_string(),
            });
            continue;
        }
        let layer = item.layer.trim().to_ascii_lowercase();
        let layer = if Layer::parse(&layer).is_some() {
            layer
        } else {
            draft
                .verified
                .iter()
                .find(|candidate| candidate.id == candidate_id)
                .map(|candidate| candidate.layer.clone())
                .unwrap_or_else(|| "fa".to_string())
        };
        let mut evidence = item.evidence;
        if evidence.is_empty() {
            evidence = draft
                .verified
                .iter()
                .find(|candidate| candidate.id == candidate_id)
                .map(|candidate| candidate.evidence.clone())
                .unwrap_or_default();
        }
        units.push(SkillUnitDraft {
            candidate_id,
            title,
            layer,
            trigger_condition: item.trigger_condition.trim().to_string(),
            steps,
            mechanism: item.mechanism.trim().to_string(),
            boundary: item.boundary.trim().to_string(),
            evidence,
        });
    }

    if units.is_empty() {
        return Err(CoreError::MalformedResponse(
            "技能单元生成后没有有效单元".to_string(),
        ));
    }
    draft.units = units;
    draft.excluded = excluded;
    Ok((DistillStage::Map, DistillState::Running, draft, 1))
}

// ---------- 阶段3 技能地图（本地计算，不调用模型） ----------

fn run_map(mut draft: DistillDraft) -> StageResult {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for unit in &draft.units {
        groups
            .entry(unit.layer.clone())
            .or_default()
            .push(unit.title.clone());
    }
    draft.skill_map = groups
        .into_iter()
        .map(|(layer, titles)| SkillGroup {
            name: Layer::parse(&layer)
                .map(|value| value.name().to_string())
                .unwrap_or_else(|| layer.clone()),
            layer,
            titles,
        })
        .collect();

    let mut links = Vec::new();
    'outer: for (index, left) in draft.units.iter().enumerate() {
        for right in draft.units.iter().skip(index + 1) {
            if links.len() >= MAX_SKILL_LINKS {
                break 'outer;
            }
            let same_layer = left.layer == right.layer;
            let shared_evidence = left
                .evidence
                .iter()
                .any(|item| right.evidence.iter().any(|other| other == item));
            if same_layer || shared_evidence {
                links.push(SkillLink {
                    from: left.title.clone(),
                    to: right.title.clone(),
                    relation: if shared_evidence {
                        "同源".to_string()
                    } else {
                        "同层".to_string()
                    },
                });
            }
        }
    }
    draft.links = links;
    (DistillStage::Stress, DistillState::Running, draft, 0)
}

// ---------- 阶段4 压力测试 ----------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawStress {
    question: String,
    decoy: bool,
    expected: String,
    answer: String,
    passed: bool,
}

fn run_stress(
    conn: &Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
    job: &DistillJobView,
    mut draft: DistillDraft,
) -> CoreResult<StageResult> {
    let system = "你在对刚蒸馏出的大师做压力测试。设计若干道题，其中一部分是诱饵题：\
诱饵题超出该大师的适用边界，正确表现是明确指出不适用。对每题给出回答与是否通过。\
严格只输出 JSON 数组，每个元素格式：{\"question\":\"题目\",\"decoy\":false,\
\"expected\":\"期望表现\",\"answer\":\"试答\",\"passed\":true}。";
    let mut listing = String::new();
    for unit in &draft.units {
        listing.push_str(&format!(
            "- {}（{}）：触发条件 {}；边界 {}\n",
            unit.title, unit.layer, unit.trigger_condition, unit.boundary
        ));
    }
    let user = format!(
        "大师：{}（领域：{}）\n技能单元：\n{listing}\n请设计压力测试题。",
        job.master_name, job.domain
    );
    let raw = call_stage(conn, client, policy, PURPOSE_STRESS, system, &user)?;
    let parsed: Vec<RawStress> = serde_json::from_str(json_array(&raw)).unwrap_or_default();
    let stress: Vec<StressCaseDraft> = parsed
        .into_iter()
        .filter(|item| !item.question.trim().is_empty())
        .map(|item| StressCaseDraft {
            question: item.question.trim().to_string(),
            decoy: item.decoy,
            expected: item.expected.trim().to_string(),
            answer: item.answer.trim().to_string(),
            passed: item.passed,
        })
        .collect();
    let pass_rate = if stress.is_empty() {
        0.0
    } else {
        stress.iter().filter(|case| case.passed).count() as f64 / stress.len() as f64
    };
    if !stress.is_empty() && pass_rate < STRESS_PASS_THRESHOLD {
        draft.excluded.push(ExcludedCandidate {
            id: "stress".to_string(),
            track: "stress".to_string(),
            title: "压力测试".to_string(),
            stage: "stress".to_string(),
            reason: format!("通过率 {:.0}% 低于阈值", pass_rate * 100.0),
        });
    }
    draft.stress = stress;
    draft.stress_pass_rate = pass_rate;
    Ok((DistillStage::Deliver, DistillState::Running, draft, 1))
}

// ---------- 阶段5 交付与安装 ----------

fn run_deliver(
    conn: &mut Connection,
    job: &DistillJobView,
    mut draft: DistillDraft,
) -> CoreResult<StageResult> {
    let materials = repo::job_materials(conn, &job.id)?;
    let pack_dir = PathBuf::from(&job.output_dir).join(&job.master_id);
    std::fs::create_dir_all(&pack_dir)?;

    // 每份材料生成一个语料引用：文件直接用绝对路径，文本型材料落到包内文件。
    let mut corpus_entries = Vec::new();
    let mut refs = Vec::new();
    for (index, material) in materials.iter().enumerate() {
        let is_file =
            !material.source_ref.trim().is_empty() && Path::new(&material.source_ref).is_file();
        let reference = if is_file {
            material.source_ref.trim().to_string()
        } else {
            let file_name = format!("corpus/{:02}-{}.txt", index + 1, safe_slug(&material.title));
            let path = pack_dir.join(&file_name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, material.text.as_bytes())?;
            file_name
        };
        refs.push((material.title.clone(), reference.clone()));
        corpus_entries.push(serde_json::json!({
            "ref": reference,
            "kind": if is_file { "file" } else { material.kind.as_str() },
            "title": material.title,
            "locationHint": material.source_ref,
        }));
    }

    let layers: Vec<String> = {
        let mut seen = BTreeSet::new();
        draft
            .units
            .iter()
            .filter(|unit| Layer::parse(&unit.layer).is_some())
            .filter(|unit| seen.insert(unit.layer.clone()))
            .map(|unit| unit.layer.clone())
            .collect()
    };

    let units_json: Vec<serde_json::Value> = draft
        .units
        .iter()
        .map(|unit| {
            let evidence: Vec<serde_json::Value> = resolve_evidence(unit, &refs);
            serde_json::json!({
                "title": unit.title,
                "layer": unit.layer,
                "triggerCondition": unit.trigger_condition,
                "steps": unit.steps,
                "mechanism": unit.mechanism,
                "boundary": unit.boundary,
                "evidence": evidence,
            })
        })
        .collect();

    let skeleton = draft.skeleton.clone().unwrap_or_default();
    let version = next_version(conn, &job.master_id)?;
    let manifest = serde_json::json!({
        "format": master::pack::PACK_FORMAT,
        "formatVersion": master::pack::PACK_FORMAT_VERSION,
        "id": job.master_id,
        "name": job.master_name,
        "domain": if skeleton.domain.trim().is_empty() { job.domain.clone() } else { skeleton.domain.clone() },
        "layers": layers,
        "version": version,
        "summary": skeleton.summary,
        "style": skeleton.angles.join("、"),
        "blindSpots": "",
        "note": format!("蒸馏自 {} 份材料", materials.len()),
        "units": units_json,
        "corpus": corpus_entries,
    });
    let manifest_path = pack_dir.join("master.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| CoreError::InvalidInput(format!("大师包序列化失败：{error}")))?,
    )?;

    let outcome = master_repo::install(conn, &pack_dir)?;
    draft.pack_dir = Some(pack_dir.to_string_lossy().to_string());
    draft.outcome = Some(outcome);
    Ok((DistillStage::Done, DistillState::Done, draft, 0))
}

/// 把技能单元的来源标注映射到已声明的语料引用。
fn resolve_evidence(
    unit: &SkillUnitDraft,
    refs: &[(String, String)],
) -> Vec<serde_json::Value> {
    let excerpt = truncated(&unit.mechanism, 80);
    if refs.is_empty() {
        return vec![serde_json::json!({
            "corpusRef": "corpus/README.txt",
            "excerpt": excerpt,
            "location": unit.title,
        })];
    }
    let mut resolved = Vec::new();
    for item in &unit.evidence {
        if let Some((title, reference)) = refs.iter().find(|(title, reference)| {
            item.contains(title.as_str()) || item.contains(reference.as_str())
        }) {
            resolved.push(serde_json::json!({
                "corpusRef": reference,
                "excerpt": excerpt,
                "location": title,
            }));
        }
    }
    if resolved.is_empty() {
        let (title, reference) = &refs[0];
        resolved.push(serde_json::json!({
            "corpusRef": reference,
            "excerpt": excerpt,
            "location": title,
        }));
    }
    resolved
}

fn next_version(conn: &Connection, master_id: &str) -> CoreResult<i64> {
    let current: Option<i64> = conn
        .query_row(
            "SELECT current_version FROM masters WHERE id = ?1",
            [master_id],
            |row| row.get(0),
        )
        .ok();
    Ok(current.map(|value| value + 1).unwrap_or(1))
}

fn safe_slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.chars().count() >= 40 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "material".to_string()
    } else {
        trimmed
    }
}

/// 供测试与界面复用：当前层的展示名。
pub fn layer_name(layer: &str) -> String {
    Layer::parse(layer)
        .map(|value| value.name().to_string())
        .unwrap_or_else(|| layer.to_string())
}
