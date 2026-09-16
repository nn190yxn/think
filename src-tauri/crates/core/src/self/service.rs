//! 自我蒸馏服务：就绪度、生成初稿、逐条确认与安装。

use std::path::Path;

use rusqlite::Connection;
use serde::Deserialize;

use crate::error::{CoreError, CoreResult};
use crate::llm::{call_model, ModelClient, ModelRequest, RetryPolicy};
use crate::master::{self, repo as master_repo, Layer};

use super::repo;
use super::{
    SelfDraftDetail, SelfDraftState, SelfReadiness, SelfRecord, MAX_ITEMS, MAX_RECORDS,
    PURPOSE_SELF, SELF_DOMAIN, SELF_MASTER_ID, SELF_MASTER_NAME, UNLOCK_RECORD_COUNT,
};

/// 相对大师包根的语料引用，自我蒸馏的全部来源都指向它。
const CORPUS_REF: &str = "corpus/self-records.txt";
/// 单条候选最多携带的来源记录数。
const MAX_EVIDENCE: usize = 3;

const INSTRUCTION: &str = "请把上面的思考记录凝练成属于这位用户自己的判断框架。\
只输出 JSON 数组，每个元素形如：\
{\"title\":\"一句话标题\",\"layer\":\"dao|fa|shu|qi|tool|shi\",\
\"triggerCondition\":\"何时触发\",\"steps\":[\"第一步\",\"第二步\"],\
\"mechanism\":\"为什么有效\",\"boundary\":\"适用边界\",\"records\":[0,3]}。\
要求四要素齐全、steps 至少一步、records 引用上面的记录序号，条目数 3 到 8 条。\
不要输出解释或代码块围栏。";

/// 模型返回的候选条目原始结构。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCandidate {
    title: String,
    layer: String,
    trigger_condition: String,
    #[serde(default)]
    steps: Vec<String>,
    mechanism: String,
    boundary: String,
    #[serde(default)]
    records: Vec<usize>,
}

/// 过滤后、尚未落库的候选条目。
struct PreparedCandidate {
    title: String,
    layer: Layer,
    trigger_condition: String,
    steps: Vec<String>,
    mechanism: String,
    boundary: String,
    records: Vec<usize>,
}

/// 铜镜入口的就绪度：记录够不够、装没装、席位开没开。
pub fn readiness(conn: &Connection) -> CoreResult<SelfReadiness> {
    let record_count = repo::record_count(conn)?;
    let version = repo::installed_version(conn)?;
    Ok(SelfReadiness {
        record_count,
        required: UNLOCK_RECORD_COUNT,
        unlocked: record_count >= UNLOCK_RECORD_COUNT,
        installed: version > 0,
        seat_enabled: repo::seat_enabled(conn)?,
        current_version: version,
        latest_draft: repo::latest_draft(conn)?,
    })
}

pub fn detail(conn: &Connection, draft_id: &str) -> CoreResult<SelfDraftDetail> {
    let draft = repo::get_draft(conn, draft_id)?;
    let items = repo::items(conn, draft_id)?;
    let (pending, accepted, rejected) = repo::item_counts(conn, draft_id)?;
    Ok(SelfDraftDetail {
        draft,
        items,
        pending_count: pending,
        accepted_count: accepted,
        rejected_count: rejected,
    })
}

pub fn latest_detail(conn: &Connection) -> CoreResult<Option<SelfDraftDetail>> {
    match repo::latest_draft(conn)? {
        Some(draft) => Ok(Some(detail(conn, &draft.id)?)),
        None => Ok(None),
    }
}

/// 启动一次自我蒸馏：读取历史记录，调用模型产出候选条目。
///
/// 未达到解锁条数时直接报错，不发起模型调用，符合「不催促」的约定。
pub fn start(
    conn: &mut Connection,
    client: &dyn ModelClient,
    policy: &RetryPolicy,
) -> CoreResult<SelfDraftDetail> {
    let count = repo::record_count(conn)?;
    if count < UNLOCK_RECORD_COUNT {
        return Err(CoreError::InvalidInput(format!(
            "思考记录达到 {UNLOCK_RECORD_COUNT} 条后才能自我蒸馏，当前为 {count} 条"
        )));
    }

    let records = repo::records(conn, MAX_RECORDS)?;
    if records.is_empty() {
        return Err(CoreError::InvalidInput("没有可用于自我蒸馏的记录".to_string()));
    }

    let draft = repo::create_draft(conn, count)?;
    let prompt = build_prompt(&records);
    let request = ModelRequest::new(PURPOSE_SELF, "你是一位严谨的判断框架提炼者。", prompt);

    let response = match call_model(conn, client, &request, policy) {
        Ok(response) => response,
        Err(error) => {
            repo::set_draft_state(
                conn,
                &draft.id,
                SelfDraftState::Failed,
                Some(error.code()),
                &error.to_string(),
                1,
            )?;
            return Err(error);
        }
    };

    let candidates = parse_candidates(&response.content);
    if candidates.is_empty() {
        repo::set_draft_state(
            conn,
            &draft.id,
            SelfDraftState::Failed,
            Some("E_MALFORMED_RESPONSE"),
            "模型没有产出可用的判断框架",
            1,
        )?;
        return Err(CoreError::MalformedResponse(
            "自我蒸馏未产出可用的判断框架".to_string(),
        ));
    }

    let tx = conn.transaction()?;
    let mut ordinal = 0i64;
    for candidate in candidates.iter().take(MAX_ITEMS) {
        let (evidence, source_record_id) = resolve_evidence(candidate, &records);
        repo::insert_item(
            &tx,
            &draft.id,
            ordinal,
            &candidate.title,
            candidate.layer,
            &candidate.trigger_condition,
            &candidate.steps,
            &candidate.mechanism,
            &candidate.boundary,
            &evidence,
            source_record_id.as_deref(),
        )?;
        ordinal += 1;
    }
    tx.commit()?;

    repo::set_draft_state(
        conn,
        &draft.id,
        SelfDraftState::Ready,
        None,
        &format!("从 {} 条记录中提炼出 {} 条待确认框架", records.len(), ordinal),
        1,
    )?;
    detail(conn, &draft.id)
}

/// 逐条确认。拒绝也记录，方便回看当时的取舍。
pub fn decide(
    conn: &Connection,
    draft_id: &str,
    item_id: &str,
    accepted: bool,
) -> CoreResult<SelfDraftDetail> {
    let draft = repo::get_draft(conn, draft_id)?;
    if draft.status == SelfDraftState::Installed.as_str() {
        return Err(CoreError::InvalidInput(
            "该草稿已安装，不能继续修改".to_string(),
        ));
    }
    repo::decide_item(conn, item_id, accepted)?;
    detail(conn, draft_id)
}

/// 安装用户本人大师包。要求所有条目都已确认，至少采纳一条。
pub fn install(
    conn: &mut Connection,
    draft_id: &str,
    output_dir: &Path,
) -> CoreResult<master::InstallOutcome> {
    let draft = repo::get_draft(conn, draft_id)?;
    if draft.status == SelfDraftState::Installed.as_str() {
        return Err(CoreError::InvalidInput(
            "该草稿已经安装，请重新发起自我蒸馏".to_string(),
        ));
    }
    let (pending, accepted_count, _rejected) = repo::item_counts(conn, draft_id)?;
    if pending > 0 {
        return Err(CoreError::InvalidInput(format!(
            "还有 {pending} 条框架没有确认"
        )));
    }
    if accepted_count == 0 {
        return Err(CoreError::InvalidInput(
            "至少采纳一条判断框架才能安装".to_string(),
        ));
    }

    let accepted = repo::accepted_items(conn, draft_id)?;
    let records = repo::records(conn, MAX_RECORDS)?;
    let pack_dir = output_dir.join("self-pack");
    std::fs::create_dir_all(pack_dir.join("corpus"))?;
    std::fs::write(pack_dir.join(CORPUS_REF), render_corpus(&records).as_bytes())?;

    let layers = distinct_layers(&accepted);
    let units: Vec<serde_json::Value> = accepted
        .iter()
        .map(|item| {
            serde_json::json!({
                "title": item.title,
                "layer": item.layer.as_str(),
                "triggerCondition": item.trigger_condition,
                "steps": item.steps,
                "mechanism": item.mechanism,
                "boundary": item.boundary,
                "evidence": [{
                    "corpusRef": CORPUS_REF,
                    "excerpt": truncated(&item.mechanism, 80),
                    "location": item.title,
                }],
            })
        })
        .collect();

    let version = repo::installed_version(conn)? + 1;
    let manifest = serde_json::json!({
        "format": master::pack::PACK_FORMAT,
        "formatVersion": master::pack::PACK_FORMAT_VERSION,
        "id": SELF_MASTER_ID,
        "name": SELF_MASTER_NAME,
        "domain": SELF_DOMAIN,
        "layers": layers.iter().map(|layer| layer.as_str()).collect::<Vec<_>>(),
        "version": version,
        "summary": "基于我自己的思考记录蒸馏出的判断框架。",
        "style": "沿用我过去的表达与取舍习惯",
        "blindSpots": "只反映记录中留下的部分，未被记录的想法不在此列。",
        "note": format!("自我蒸馏第 {version} 版，采纳 {} 条框架", accepted.len()),
        "units": units,
        "corpus": [{
            "ref": CORPUS_REF,
            "kind": "text",
            "title": "我的思考记录",
            "locationHint": "本机思考记录",
        }],
    });
    std::fs::write(
        pack_dir.join("master.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| CoreError::InvalidInput(format!("大师包序列化失败：{error}")))?,
    )?;

    let outcome = master_repo::install(conn, &pack_dir)?;
    repo::set_draft_state(
        conn,
        draft_id,
        SelfDraftState::Installed,
        None,
        &format!("已安装为我 v{}", outcome.version),
        draft.model_calls,
    )?;
    repo::set_seat_enabled(conn, true)?;
    Ok(outcome)
}

/// 会诊保留席位的判定：已安装且席位开启时返回「你」的大师标识。
pub fn seat_master_id(conn: &Connection) -> CoreResult<Option<String>> {
    let installed = repo::installed_version(conn)? > 0;
    if installed && repo::seat_enabled(conn)? {
        Ok(Some(SELF_MASTER_ID.to_string()))
    } else {
        Ok(None)
    }
}

pub fn set_seat_enabled(conn: &Connection, enabled: bool) -> CoreResult<SelfReadiness> {
    repo::set_seat_enabled(conn, enabled)?;
    readiness(conn)
}

fn build_prompt(records: &[SelfRecord]) -> String {
    let mut text = String::from(
        "以下是一位用户在过去会诊中形成的思考记录，按时间倒序排列。\
         每条记录包含议题、当时的结论与处置。\n\n",
    );
    for (index, record) in records.iter().enumerate() {
        text.push_str(&format!(
            "[{index}] 议题：{}（{}）\n结论：{}\n处置：{}",
            record.question,
            record.created_at,
            record.conclusion,
            if record.adopted { "已采纳" } else { "未标注" }
        ));
        if !record.reason.trim().is_empty() {
            text.push_str(&format!("\n理由：{}", record.reason));
        }
        text.push_str("\n\n");
    }
    text.push_str(INSTRUCTION);
    text
}

/// 过滤掉四要素不全或层次非法的候选，只保留可安装的条目。
fn parse_candidates(raw: &str) -> Vec<PreparedCandidate> {
    let slice = match (raw.find('['), raw.rfind(']')) {
        (Some(start), Some(end)) if end > start => &raw[start..=end],
        _ => return Vec::new(),
    };
    let parsed: Vec<RawCandidate> = match serde_json::from_str(slice) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    parsed
        .into_iter()
        .take(MAX_ITEMS)
        .filter_map(|candidate| {
            let title = candidate.title.trim().to_string();
            let layer = Layer::parse(candidate.layer.trim())?;
            let steps: Vec<String> = candidate
                .steps
                .iter()
                .map(|step| step.trim().to_string())
                .filter(|step| !step.is_empty())
                .collect();
            if title.is_empty()
                || candidate.trigger_condition.trim().is_empty()
                || steps.is_empty()
                || candidate.mechanism.trim().is_empty()
                || candidate.boundary.trim().is_empty()
            {
                return None;
            }
            Some(PreparedCandidate {
                title,
                layer,
                trigger_condition: candidate.trigger_condition.trim().to_string(),
                steps,
                mechanism: candidate.mechanism.trim().to_string(),
                boundary: candidate.boundary.trim().to_string(),
                records: candidate.records,
            })
        })
        .collect()
}

/// 把模型给出的记录序号映射成可读来源；序号无效时退回最近一条，保证可追溯。
fn resolve_evidence(candidate: &PreparedCandidate, records: &[SelfRecord]) -> (Vec<String>, Option<String>) {
    let mut evidence = Vec::new();
    let mut source: Option<String> = None;
    for index in candidate.records.iter().take(MAX_EVIDENCE) {
        if let Some(record) = records.get(*index) {
            evidence.push(format!(
                "[{}] {}",
                record.question,
                truncated(&record.conclusion, 60)
            ));
            if source.is_none() {
                source = Some(record.id.clone());
            }
        }
    }
    if evidence.is_empty() {
        if let Some(record) = records.first() {
            evidence.push(format!(
                "[{}] {}",
                record.question,
                truncated(&record.conclusion, 60)
            ));
            source = Some(record.id.clone());
        }
    }
    (evidence, source)
}

fn render_corpus(records: &[SelfRecord]) -> String {
    let mut text = String::from("我的思考记录（自我蒸馏来源）\n\n");
    for record in records {
        text.push_str(&format!(
            "议题：{}\n结论：{}\n处置：{}\n\n",
            record.question,
            record.conclusion,
            if record.adopted { "已采纳" } else { "未标注" }
        ));
    }
    text
}

fn distinct_layers(items: &[super::SelfItemView]) -> Vec<Layer> {
    let mut layers: Vec<Layer> = Vec::new();
    for item in items {
        if !layers.contains(&item.layer) {
            layers.push(item.layer);
        }
    }
    layers.sort();
    layers
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
