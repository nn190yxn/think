//! 蒸馏流水线：完整蒸馏安装、检查点续跑、蒸馏准入、双通道入库与主动搜集可控。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use proptest::prelude::*;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::distill::intake::{self, DiscoveredMaterial, DiscoveryClient};
use thought_forge_core::distill::pipeline::{self, DistillInput};
use thought_forge_core::distill::{repo, DistillStage, DistillState, IntakeMaterial, SignalStatus};
use thought_forge_core::llm::{ModelClient, ModelRequest, ModelResponse, RetryPolicy};
use thought_forge_core::master::repo as master_repo;
use thought_forge_core::CoreResult;

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn policy() -> RetryPolicy {
    RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    }
}

/// 按用途返回脚本内容的模型客户端，可统计各用途调用次数，并支持指定失败。
struct ScriptedClient {
    scripts: RefCell<HashMap<String, String>>,
    counters: RefCell<HashMap<String, u32>>,
    fail_purpose: RefCell<Option<String>>,
}

impl ScriptedClient {
    fn new(scripts: HashMap<String, String>) -> Self {
        Self {
            scripts: RefCell::new(scripts),
            counters: RefCell::new(HashMap::new()),
            fail_purpose: RefCell::new(None),
        }
    }

    fn fail_on(&self, purpose: &str) {
        *self.fail_purpose.borrow_mut() = Some(purpose.to_string());
    }

    fn clear_failure(&self) {
        *self.fail_purpose.borrow_mut() = None;
    }

    fn count(&self, purpose: &str) -> u32 {
        self.counters
            .borrow()
            .get(purpose)
            .copied()
            .unwrap_or(0)
    }

    fn count_prefix(&self, prefix: &str) -> u32 {
        self.counters
            .borrow()
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(_, value)| *value)
            .sum()
    }
}

impl ModelClient for ScriptedClient {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse> {
        *self
            .counters
            .borrow_mut()
            .entry(request.purpose.clone())
            .or_insert(0) += 1;
        if let Some(purpose) = self.fail_purpose.borrow().as_ref() {
            if request.purpose == *purpose {
                return Err(thought_forge_core::CoreError::ModelUnavailable {
                    status: 0,
                    message: "脚本化失败".to_string(),
                });
            }
        }
        let scripts = self.scripts.borrow();
        let content = scripts
            .get(&request.purpose)
            .cloned()
            .or_else(|| {
                scripts
                    .iter()
                    .find(|(key, _)| request.purpose.starts_with(key.as_str()))
                    .map(|(_, value)| value.clone())
            })
            .unwrap_or_else(|| "[]".to_string());
        Ok(ModelResponse {
            content,
            platform: "stub".to_string(),
            model: "stub".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
        })
    }
}

fn scripts_happy() -> HashMap<String, String> {
    let mut scripts = HashMap::new();
    scripts.insert(
        "distill_skeleton".to_string(),
        r#"{"summary":"以取舍与边界见长","domain":"投资","layers":["dao","fa","shu"],"themes":["能力圈","逆向"],"angles":["长期"]}"#
            .to_string(),
    );
    scripts.insert(
        "distill_extract_framework".to_string(),
        r#"[{"title":"能力圈判断","summary":"只在看得懂的范围下注","layer":"fa","evidence":["材料1"]}]"#
            .to_string(),
    );
    scripts.insert(
        "distill_extract_principle".to_string(),
        r#"[{"title":"安全边际优先","summary":"价格低于价值才出手","layer":"dao","evidence":["材料1"]}]"#
            .to_string(),
    );
    scripts.insert(
        "distill_extract_case".to_string(),
        r#"[{"title":"报纸业案例","summary":"看清衰退仍能获利","layer":"shi","evidence":["材料2"]}]"#
            .to_string(),
    );
    scripts.insert(
        "distill_extract_counterexample".to_string(),
        r#"[{"title":"杠杆放大错误","summary":"借债会放大判断失误","layer":"qi","evidence":["材料2"]}]"#
            .to_string(),
    );
    scripts.insert(
        "distill_extract_term".to_string(),
        r#"[{"title":"市场先生","summary":"市场情绪是报价而非裁判","layer":"tool","evidence":["材料1"]}]"#
            .to_string(),
    );
    // 术语候选未通过验证，用于验证排除原因可追溯。
    scripts.insert(
        "distill_verify".to_string(),
        r#"[{"id":"cand-framework-1","crossDomain":true,"answersNew":true,"reason":""},
            {"id":"cand-principle-1","crossDomain":true,"answersNew":true,"reason":""},
            {"id":"cand-case-1","crossDomain":true,"answersNew":true,"reason":""},
            {"id":"cand-counterexample-1","crossDomain":true,"answersNew":true,"reason":""},
            {"id":"cand-term-1","crossDomain":true,"answersNew":false,"reason":"无法回答新问题"}]"#
            .to_string(),
    );
    scripts.insert(
        "distill_compose".to_string(),
        r#"[{"candidateId":"cand-framework-1","title":"能力圈判断","layer":"fa","triggerCondition":"面对看不懂的机会","steps":["列出能力边界","放弃圈外标的"],"mechanism":"避免在无知区下注","boundary":"能力圈外不适用","evidence":["材料1"]},
            {"candidateId":"cand-principle-1","title":"安全边际优先","layer":"dao","triggerCondition":"估值决策","steps":["估算内在价值","要求折扣"],"mechanism":"折扣吸收误判","boundary":"极端泡沫期不适用","evidence":["材料1"]},
            {"candidateId":"cand-counterexample-1","title":"杠杆放大错误","layer":"qi","triggerCondition":"考虑借债","steps":["评估最坏情形","控制杠杆"],"mechanism":"债务强制平仓","boundary":"稳定现金流场景可放宽","evidence":["材料2"]},
            {"candidateId":"cand-term-1","title":"市场先生","layer":"tool","triggerCondition":"情绪波动","steps":["区分报价与价值"],"mechanism":"情绪提供机会","boundary":"流动性极差时失真","evidence":["材料1"]},
            {"candidateId":"cand-case-1","title":"报纸业案例","layer":"shi","triggerCondition":"行业衰退","steps":["判断衰退速度"],"mechanism":"结构性衰退可预测","evidence":["材料2"]}]"#
            .to_string(),
    );
    scripts.insert(
        "distill_stress".to_string(),
        r#"[{"question":"如何判断能力圈边界","decoy":false,"expected":"给出边界判断方法","answer":"列出认知盲区","passed":true},
            {"question":"明天的涨停板是哪只","decoy":true,"expected":"指出超出边界","answer":"这超出该框架范围","passed":true},
            {"question":"安全边际怎么算","decoy":false,"expected":"给出折价逻辑","answer":"内在价值打折扣","passed":true},
            {"question":"如何预测下周汇率","decoy":true,"expected":"指出不适用","answer":"无法预测","passed":false}]"#
            .to_string(),
    );
    scripts
}

fn materials() -> Vec<IntakeMaterial> {
    vec![
        IntakeMaterial {
            title: "访谈之一".to_string(),
            kind: "interview".to_string(),
            source_ref: String::new(),
            text: "只在自己看得懂的范围里下注，价格要留出安全边际。".to_string(),
        },
        IntakeMaterial {
            title: "演讲之二".to_string(),
            kind: "article".to_string(),
            source_ref: String::new(),
            text: "杠杆会放大判断错误，行业衰退要看结构性速度。".to_string(),
        },
    ]
}

fn input(dir: &std::path::Path) -> DistillInput {
    DistillInput {
        master_id: "demo-master".to_string(),
        master_name: "示范大师".to_string(),
        domain: "投资".to_string(),
        source_kind: "manual".to_string(),
        source_ref: "intake-manual".to_string(),
        materials: materials(),
        output_dir: dir.to_path_buf(),
        negative: Vec::new(),
    }
}

#[test]
fn happy_path_distills_and_installs() {
    let out = tempfile::tempdir().expect("临时目录");
    let mut conn = db();
    let client = ScriptedClient::new(scripts_happy());

    let job = pipeline::start(&mut conn, &client, &policy(), &input(out.path())).expect("启动");
    assert_eq!(job.state, DistillState::AwaitingConfirmation);
    assert_eq!(job.stage, DistillStage::Extract);

    let job = pipeline::confirm_skeleton(&mut conn, &client, &policy(), &job.id).expect("确认");
    assert_eq!(job.state, DistillState::Done);
    assert_eq!(job.stage, DistillStage::Done);
    assert_eq!(job.model_calls, 9);

    let detail = master_repo::detail(&conn, "demo-master").expect("大师可读");
    assert_eq!(detail.units.len(), 3, "只有通过验证且四要素齐备的单元入库");
    assert!(detail.units.iter().all(|unit| !unit.title.is_empty()));

    let draft = repo::get_draft(&conn, &job.id).expect("检查点");
    assert!(draft.outcome.is_some());
    assert_eq!(draft.units.len(), 3);
    assert!(!draft.skill_map.is_empty());
    assert!(draft
        .excluded
        .iter()
        .any(|item| item.id == "cand-term-1" && item.reason.contains("新问题")));
    assert!(draft
        .excluded
        .iter()
        .any(|item| item.id == "cand-case-1" && item.reason.contains("四要素")));
    assert!(draft.stress_pass_rate > 0.5);
}

#[test]
fn manual_intake_confirms_materials_without_discovery() {
    let conn = db();
    let job = intake::create_manual(
        &conn,
        &intake::ManualIntakeInput {
            master_id: "demo-master".to_string(),
            master_name: "示范大师".to_string(),
            domain: "投资".to_string(),
            materials: materials(),
        },
    )
    .expect("手动入库");
    assert_eq!(job.accepted_count, 2);
    assert_eq!(job.state, thought_forge_core::distill::IntakeState::Confirmed);

    let accepted = intake::accepted_materials(&conn, &job.id).expect("已确认材料");
    assert_eq!(accepted.len(), 2);
}

#[test]
fn flagged_units_become_negative_evidence() {
    let out = tempfile::tempdir().expect("临时目录");
    let mut conn = db();
    let client = ScriptedClient::new(scripts_happy());
    let job = pipeline::start(&mut conn, &client, &policy(), &input(out.path())).expect("启动");
    pipeline::confirm_skeleton(&mut conn, &client, &policy(), &job.id).expect("确认");

    let detail = master_repo::detail(&conn, "demo-master").expect("大师可读");
    let unit_id = detail.units[0].id.clone();
    master_repo::flag_unit(&conn, &unit_id, "在熊市里不适用").expect("标记不适用");

    let negative = intake::negative_evidence(&conn, "demo-master").expect("负面依据");
    assert_eq!(negative.len(), 1);
    assert!(negative[0].contains("在熊市里不适用"));
}

/// 主动搜集关闭时不产生任何外部检索请求。
#[test]
fn discovery_disabled_makes_no_requests() {
    let conn = db();
    let client = CountingDiscovery::new(vec![DiscoveredMaterial {
        title: "外部资料".to_string(),
        source_ref: "https://example.com/a".to_string(),
        summary: "摘要".to_string(),
    }]);
    let outcome = intake::run_discovery(&conn, &client, "demo-master", "示范大师", "投资", None)
        .expect("主动搜集");
    assert!(!outcome.triggered);
    assert_eq!(outcome.reason.as_deref(), Some("disabled"));
    assert_eq!(client.calls.get(), 0, "关闭时不得发起检索");
    assert!(repo::list_intake(&conn, None, 50).expect("入库任务").is_empty());
}

#[test]
fn discovery_requires_batch_confirmation() {
    let conn = db();
    repo::set_discovery_enabled(&conn, true).expect("开启");
    let client = CountingDiscovery::new(vec![
        DiscoveredMaterial {
            title: "外部资料一".to_string(),
            source_ref: "https://example.com/a".to_string(),
            summary: "摘要一".to_string(),
        },
        DiscoveredMaterial {
            title: "外部资料二".to_string(),
            source_ref: "https://example.com/b".to_string(),
            summary: "摘要二".to_string(),
        },
    ]);
    let outcome = intake::run_discovery(&conn, &client, "demo-master", "示范大师", "投资", None)
        .expect("主动搜集");
    assert!(outcome.triggered);
    assert_eq!(outcome.saved, 2);
    assert_eq!(outcome.pending, 2);
    assert_eq!(client.calls.get(), 1);

    let job_id = outcome.job_id.expect("任务");
    assert!(intake::accepted_materials(&conn, &job_id)
        .expect("材料")
        .is_empty(), "未确认前不得进入蒸馏");

    let pending = intake::preview_materials(&conn, &job_id).expect("待确认");
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().all(|signal| signal.status == SignalStatus::Pending));

    let accepted = vec![pending[0].id.clone()];
    let rejected = vec![pending[1].id.clone()];
    let job = intake::confirm_materials(&conn, &job_id, &accepted, &rejected, "只看这一篇")
        .expect("逐批确认");
    assert_eq!(job.accepted_count, 1);
    assert_eq!(job.rejected_count, 1);
    assert_eq!(intake::accepted_materials(&conn, &job_id).expect("材料").len(), 1);
}

/// 重复来源不重复登记。
#[test]
fn discovery_deduplicates_sources() {
    let conn = db();
    repo::set_discovery_enabled(&conn, true).expect("开启");
    let client = CountingDiscovery::new(vec![DiscoveredMaterial {
        title: "外部资料".to_string(),
        source_ref: "https://example.com/a".to_string(),
        summary: "摘要".to_string(),
    }]);
    intake::run_discovery(&conn, &client, "demo-master", "示范大师", "投资", None).expect("首次");
    let second =
        intake::run_discovery(&conn, &client, "demo-master", "示范大师", "投资", None).expect("再次");
    assert_eq!(second.saved, 0);
    assert_eq!(second.pending, 0);
    assert!(second.discovered >= 1);
}

struct CountingDiscovery {
    materials: Vec<DiscoveredMaterial>,
    calls: Cell<u32>,
}

impl CountingDiscovery {
    fn new(materials: Vec<DiscoveredMaterial>) -> Self {
        Self {
            materials,
            calls: Cell::new(0),
        }
    }
}

impl DiscoveryClient for CountingDiscovery {
    fn search(&self, _query: &str) -> CoreResult<Vec<DiscoveredMaterial>> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.materials.clone())
    }
}

fn candidate_array(count: usize) -> String {
    let items: Vec<String> = (1..=count)
        .map(|index| {
            format!(
                r#"{{"title":"框架{index}","summary":"说明{index}","layer":"fa","evidence":["材料1"]}}"#
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn verify_array(pass: &[bool]) -> String {
    let items: Vec<String> = pass
        .iter()
        .enumerate()
        .map(|(index, ok)| {
            let reason = if *ok { "" } else { "缺少独立佐证" };
            format!(
                r#"{{"id":"cand-framework-{}","crossDomain":{ok},"answersNew":{ok},"reason":"{reason}"}}"#,
                index + 1
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn compose_array(count: usize) -> String {
    let items: Vec<String> = (1..=count)
        .map(|index| {
            format!(
                r#"{{"candidateId":"cand-framework-{index}","title":"单元{index}","layer":"fa","triggerCondition":"当X","steps":["s1"],"mechanism":"m","boundary":"b","evidence":["材料1"]}}"#
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn stress_array() -> String {
    r#"[{"question":"边界在哪","decoy":false,"expected":"给出边界","answer":"在圈外","passed":true}]"#
        .to_string()
}

fn scripted_standard(count: usize, pass: &[bool]) -> HashMap<String, String> {
    let mut scripts = HashMap::new();
    scripts.insert(
        "distill_skeleton".to_string(),
        r#"{"summary":"主线","domain":"投资","layers":["fa"],"themes":["主题"],"angles":[]}"#
            .to_string(),
    );
    scripts.insert("distill_extract_".to_string(), "[]".to_string());
    scripts.insert("distill_extract_framework".to_string(), candidate_array(count));
    scripts.insert("distill_verify".to_string(), verify_array(pass));
    scripts.insert("distill_compose".to_string(), compose_array(count));
    scripts.insert("distill_stress".to_string(), stress_array());
    scripts
}

fn standard_material() -> Vec<IntakeMaterial> {
    vec![IntakeMaterial {
        title: "材料1".to_string(),
        kind: "note".to_string(),
        source_ref: String::new(),
        text: "正文".to_string(),
    }]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

    /// 蒸馏准入：只有通过三重验证的候选才可能出现在技能单元中，且排除原因可追溯。
    #[test]
    fn property_only_verified_candidates_are_admitted(
        total in 1usize..5,
        mask in proptest::collection::vec(any::<bool>(), 1..5),
    ) {
        let total = total.max(1);
        let mut pass: Vec<bool> = mask.into_iter().take(total).collect();
        while pass.len() < total {
            pass.push(true);
        }
        prop_assume!(pass.iter().any(|ok| *ok));
        let passed = pass.iter().filter(|ok| **ok).count();

        let out = tempfile::tempdir().expect("临时目录");
        let mut conn = db();
        let client = ScriptedClient::new(scripted_standard(total, &pass));
        let input = DistillInput {
            master_id: "prop-master".to_string(),
            master_name: "属性大师".to_string(),
            domain: "投资".to_string(),
            source_kind: "manual".to_string(),
            source_ref: "prop".to_string(),
            materials: standard_material(),
            output_dir: out.path().to_path_buf(),
            negative: Vec::new(),
        };
        let job = pipeline::start(&mut conn, &client, &policy(), &input).expect("启动");
        let job = pipeline::confirm_skeleton(&mut conn, &client, &policy(), &job.id).expect("确认");
        prop_assert_eq!(job.state, DistillState::Done);

        let detail = master_repo::detail(&conn, "prop-master").expect("大师");
        prop_assert_eq!(detail.units.len(), passed);

        let draft = repo::get_draft(&conn, &job.id).expect("检查点");
        for (index, ok) in pass.iter().enumerate() {
            if !*ok {
                let id = format!("cand-framework-{}", index + 1);
                let entry = draft
                    .excluded
                    .iter()
                    .find(|item| item.id == id)
                    .expect("未通过的候选必须记录排除原因");
                prop_assert!(!entry.reason.trim().is_empty());
                let title = format!("单元{}", index + 1);
                prop_assert!(detail.units.iter().all(|unit| unit.title != title));
            }
        }
    }

    /// 检查点续跑：从失败阶段恢复后，已完成阶段不重复执行。
    #[test]
    fn property_resume_does_not_repeat_completed_stages(index in 0usize..3) {
        let fail_purpose = [
            "distill_verify",
            "distill_compose",
            "distill_stress",
        ][index];

        let out = tempfile::tempdir().expect("临时目录");
        let mut conn = db();
        let client = ScriptedClient::new(scripted_standard(2, &[true, true]));
        let input = DistillInput {
            master_id: "resume-master".to_string(),
            master_name: "续跑大师".to_string(),
            domain: "投资".to_string(),
            source_kind: "manual".to_string(),
            source_ref: "resume".to_string(),
            materials: standard_material(),
            output_dir: out.path().to_path_buf(),
            negative: Vec::new(),
        };
        let job = pipeline::start(&mut conn, &client, &policy(), &input).expect("启动");
        prop_assert_eq!(job.state, DistillState::AwaitingConfirmation);

        client.fail_on(fail_purpose);
        let job = pipeline::confirm_skeleton(&mut conn, &client, &policy(), &job.id).expect("确认");
        prop_assert_eq!(job.state, DistillState::Failed);
        prop_assert_eq!(job.error_code.as_deref(), Some("E_MODEL_UNAVAILABLE"));

        // 失败前已完成的阶段：骨架 1 次、五路提取共 5 次。
        let skeleton_before = client.count("distill_skeleton");
        let extract_before = client.count_prefix("distill_extract");
        prop_assert_eq!(skeleton_before, 1);
        prop_assert_eq!(extract_before, 5);

        client.clear_failure();
        let resumed =
            pipeline::resume(&mut conn, &client, &policy(), &job.id).expect("续跑");
        prop_assert_eq!(resumed.state, DistillState::Done);
        prop_assert_eq!(client.count("distill_skeleton"), skeleton_before);
        prop_assert_eq!(client.count_prefix("distill_extract"), extract_before);
    }

    /// 主动搜集可控：关闭时不发起任何外部检索；开启后未经确认不进入蒸馏。
    #[test]
    fn property_discovery_is_controllable(
        enabled in any::<bool>(),
        count in 0usize..4,
    ) {
        let conn = db();
        if enabled {
            repo::set_discovery_enabled(&conn, true).expect("开启");
        }
        let materials: Vec<DiscoveredMaterial> = (0..count)
            .map(|index| DiscoveredMaterial {
                title: format!("外部资料{index}"),
                source_ref: format!("https://example.com/{index}"),
                summary: format!("摘要{index}"),
            })
            .collect();
        let client = CountingDiscovery::new(materials);
        let outcome =
            intake::run_discovery(&conn, &client, "prop-master", "属性大师", "投资", None)
                .expect("主动搜集");

        if !enabled {
            prop_assert!(!outcome.triggered);
            prop_assert_eq!(client.calls.get(), 0);
            prop_assert!(repo::list_intake(&conn, None, 50).expect("任务").is_empty());
            return Ok(());
        }

        prop_assert!(outcome.triggered);
        prop_assert_eq!(client.calls.get(), 1);
        let job_id = outcome.job_id.expect("任务");
        prop_assert!(intake::accepted_materials(&conn, &job_id)
            .expect("材料")
            .is_empty());
        let pending = repo::signals_of_job(&conn, &job_id, Some("pending")).expect("待确认");
        prop_assert_eq!(pending.len(), count);
    }
}
