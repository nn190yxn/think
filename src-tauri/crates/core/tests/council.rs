//! 骑士团会诊：选角、换批、隔离作答与调用审计。

use std::cell::RefCell;
use std::path::PathBuf;

use thought_forge_core::council::{
    orchestrator, pairings, pool, repo, select, speech, CandidatePool, Strategy,
};
use thought_forge_core::connector::{service::Retrieval, SearchHit, SearchProvider};
use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::{ModelClient, ModelRequest, ModelResponse, RetryPolicy};
use thought_forge_core::master::repo as masters;
use thought_forge_core::{CoreError, CoreResult};

/// 脚本化模型客户端：记录每一次请求，按大师名生成可预测的答案。
struct ScriptedClient {
    calls: RefCell<Vec<ModelRequest>>,
    always_fail: bool,
}

impl ScriptedClient {
    fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            always_fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            always_fail: true,
        }
    }
}

fn speaker(system: &str) -> String {
    system
        .split('「')
        .nth(1)
        .and_then(|rest| rest.split('」').next())
        .unwrap_or("编排方")
        .to_string()
}

impl ModelClient for ScriptedClient {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse> {
        self.calls.borrow_mut().push(request.clone());
        if self.always_fail {
            return Err(thought_forge_core::CoreError::ModelUnavailable {
                status: 500,
                message: "脚本化失败".to_string(),
            });
        }
        let name = speaker(&request.system);
        Ok(ModelResponse {
            content: format!(
                "{name}的判断：先看动机与边界，再谈取舍；需要结合时机判断，暂时保留未决部分。"
            ),
            platform: "scripted".to_string(),
            model: "scripted-1".to_string(),
            prompt_tokens: 10,
            completion_tokens: 20,
        })
    }
}

fn seed_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../seed-packs")
}

/// 装好六个种子大师并预计算对立度。
fn seeded_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(seed_root())
        .expect("种子目录应存在")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.join("master.json").is_file())
        .collect();
    dirs.sort();
    for dir in dirs {
        masters::install(&mut conn, &dir).expect("种子包可安装");
    }
    pairings::recompute(&conn).expect("对立度可重算");
    conn
}

fn question() -> &'static str {
    "要不要把一个稳定的工作换成独立做产品，怎么判断值不值得"
}

/// 造一批覆盖全部六层的补位大师。
///
/// 换批要「全员换新」，前提是候选池比席位数大；种子库只有六位大师且阵容也是六席，
/// 此时换批无新人可用。这些补位大师让轮换测试能验证真正的换人行为。
fn install_extra_masters(conn: &mut rusqlite::Connection, count: usize) -> Vec<tempfile::TempDir> {
    let layers = ["dao", "fa", "shu", "qi", "tool", "shi"];
    let mut dirs = Vec::new();
    for index in 0..count {
        let dir = tempfile::tempdir().expect("临时目录");
        let manifest = serde_json::json!({
            "format": "thought-forge.master-pack",
            "formatVersion": 1,
            "id": format!("aux-{index:02}"),
            "name": format!("补位大师{index}"),
            "domain": "通用判断",
            "layers": layers,
            "version": 1,
            "summary": "换批测试用补位大师",
            "style": "直接",
            "blindSpots": "层次覆盖过宽",
            "note": "测试用",
            "units": [{
                "title": "通用取舍",
                "layer": "fa",
                "triggerCondition": "当需要判断一件事是否值得做时",
                "steps": ["先看动机", "再看边界", "最后定取舍"],
                "mechanism": "把判断拆成动机与边界",
                "boundary": "适用于一般取舍，不适用于纯技术细节",
                "evidence": [{
                    "corpusRef": "corpus/notes.md",
                    "excerpt": "先看动机，再看边界",
                    "location": "全篇"
                }]
            }],
            "corpus": [{
                "ref": "corpus/notes.md",
                "kind": "book",
                "title": "补位大师笔记",
                "locationHint": "全篇"
            }]
        });
        std::fs::write(
            dir.path().join("master.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let corpus_dir = dir.path().join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        std::fs::write(corpus_dir.join("notes.md"), "先看动机，再看边界。\n").unwrap();
        masters::install(conn, dir.path()).expect("补位大师可安装");
        dirs.push(dir);
    }
    pairings::recompute(conn).expect("对立度可重算");
    dirs
}

fn build_pool(conn: &rusqlite::Connection) -> CandidatePool {
    pool::build(
        conn,
        &pool::TopicInput {
            question: question(),
            domains: &[],
        },
    )
    .expect("候选池可构建")
}

#[test]
fn pairings_are_precomputed_for_every_master_pair() {
    let conn = seeded_db();
    let map = pairings::load(&conn).expect("可读取对立度");
    assert_eq!(map.len(), 15, "六位大师共 15 对");
    for score in map.values() {
        assert!(*score >= 0.0 && *score <= 1.0, "对立度应落在 0 到 1 之间");
    }
}

#[test]
fn default_selection_covers_all_six_layers() {
    let conn = seeded_db();
    let pool = build_pool(&conn);
    let request = select::SelectionRequest::new(Strategy::Steady);
    let plan = select::select_panel(&conn, &pool, &request).expect("可完成选角");

    assert_eq!(plan.seats.len(), 6, "默认六席");
    assert_eq!(plan.layers.len(), 6, "六层各取一位");
    assert!(plan.gaps.is_empty(), "种子库下不应有空缺");
}

#[test]
fn selection_is_deterministic_for_same_pool_and_strategy() {
    let conn = seeded_db();
    let pool = build_pool(&conn);

    for strategy in [Strategy::Steady, Strategy::Clash, Strategy::Serendipity] {
        let request = select::SelectionRequest::new(strategy);
        let first = select::select_panel(&conn, &pool, &request).expect("首次选角");
        let second = select::select_panel(&conn, &pool, &request).expect("再次选角");
        assert_eq!(
            first.master_ids(),
            second.master_ids(),
            "{strategy:?} 策略两次选角应一致"
        );
        assert_eq!(first.layers, second.layers);
    }
}

#[test]
fn panel_seats_are_persisted_and_drive_speech() {
    let conn = seeded_db();
    let pool = build_pool(&conn);
    let session =
        repo::create_session(&conn, question(), &pool.domains, &[], Strategy::Steady).unwrap();
    let plan =
        select::select_panel(&conn, &pool, &select::SelectionRequest::new(Strategy::Steady))
            .unwrap();
    repo::record_panel(&conn, &session, 0, &plan, &[]).unwrap();

    let panel = repo::latest_panel(&conn, &session).unwrap().expect("阵容已写入");
    assert_eq!(panel.seats.len(), panel.master_ids.len(), "每个席位都有指派");
    // 指派到的题必须落在这位大师声明的层次内，否则提示词会指向他没准备的题。
    for seat in &panel.seats {
        let detail = masters::detail(&conn, &seat.master_id).unwrap();
        assert!(
            detail.layers.contains(&seat.layer),
            "{} 收到 {} 层的指派，但只声明了 {:?}",
            seat.master_id,
            seat.layer,
            detail.layers
        );
    }

    // 逐席发言的题与阵容记录一致，圆桌与逐席列表不再两套口径。
    for seat in speech::seat_speech(&conn, &session, 0).unwrap() {
        assert_eq!(seat.layer, panel.layer_of(&seat.master_id).unwrap());
    }
}

#[test]
fn historical_panel_without_seats_falls_back_to_master_layers() {
    let conn = seeded_db();
    let pool = build_pool(&conn);
    let session =
        repo::create_session(&conn, question(), &pool.domains, &[], Strategy::Steady).unwrap();
    let plan =
        select::select_panel(&conn, &pool, &select::SelectionRequest::new(Strategy::Steady))
            .unwrap();
    repo::record_panel(&conn, &session, 0, &plan, &[]).unwrap();
    // 模拟升级前的历史阵容：指派列为空，读取时应回退到大师层次。
    conn.execute(
        "UPDATE council_panels SET seats_json = '[]' WHERE session_id = ?1",
        [session.as_str()],
    )
    .unwrap();

    let panel = repo::latest_panel(&conn, &session).unwrap().unwrap();
    assert_eq!(panel.seats.len(), panel.master_ids.len());
    for seat in &panel.seats {
        let detail = masters::detail(&conn, &seat.master_id).unwrap();
        assert_eq!(
            seat.layer,
            thought_forge_core::council::primary_layer(&detail.layers)
        );
    }
}

#[test]
fn first_round_prompt_names_the_assigned_question() {
    let conn = seeded_db();
    let pool = build_pool(&conn);
    let session =
        repo::create_session(&conn, question(), &pool.domains, &[], Strategy::Steady).unwrap();
    let plan =
        select::select_panel(&conn, &pool, &select::SelectionRequest::new(Strategy::Steady))
            .unwrap();
    repo::record_panel(&conn, &session, 0, &plan, &[]).unwrap();

    let client = ScriptedClient::new();
    let policy = RetryPolicy {
        attempts: 2,
        base_delay_ms: 0,
    };
    orchestrator::run_council(&conn, &client, &session, &policy).expect("会诊可跑通");

    let panel = repo::latest_panel(&conn, &session).unwrap().unwrap();
    let calls = client.calls.borrow();
    for seat in &panel.seats {
        let master = masters::detail(&conn, &seat.master_id).unwrap();
        let request = calls
            .iter()
            .find(|request| {
                request.purpose == "council_round1" && request.system.contains(&master.name)
            })
            .unwrap_or_else(|| panic!("{} 应有第一轮独立作答", master.name));
        assert!(
            request.system.contains(seat.layer.question()),
            "{} 的第一轮提示词应点名被指派的题「{}」",
            master.name,
            seat.layer.question()
        );
        assert!(
            request
                .user
                .contains(&format!("{}（{}）", seat.layer.name(), seat.layer.question())),
            "{} 的第一轮提示词应把题写进正文",
            master.name
        );
    }
}

#[test]
fn every_strategy_keeps_layer_coverage() {
    let conn = seeded_db();
    let pool = build_pool(&conn);
    for strategy in [Strategy::Steady, Strategy::Clash, Strategy::Serendipity] {
        let request = select::SelectionRequest::new(strategy);
        let plan = select::select_panel(&conn, &pool, &request).expect("可完成选角");
        assert!(
            plan.layers.len() >= thought_forge_core::council::MIN_LAYER_COVERAGE,
            "{strategy:?} 至少覆盖三个层次"
        );
    }
}

#[test]
fn pinned_seat_survives_rotation_and_rotation_keeps_coverage() {
    let mut conn = seeded_db();
    let _extra = install_extra_masters(&mut conn, 12);
    let pool = build_pool(&conn);
    let pinned = vec!["sun-tzu".to_string()];

    let initial = select::select_panel(
        &conn,
        &pool,
        &select::SelectionRequest {
            strategy: Strategy::Steady,
            size: 6,
            pinned: &pinned,
            exclude: &[],
        },
    )
    .expect("首次选角");
    assert!(initial.master_ids().contains(&"sun-tzu".to_string()));

    let current = initial.master_ids();
    let rotated = select::select_panel(
        &conn,
        &pool,
        &select::SelectionRequest {
            strategy: Strategy::Clash,
            size: 6,
            pinned: &pinned,
            exclude: &current,
        },
    )
    .expect("换批选角");

    assert!(
        rotated.master_ids().contains(&"sun-tzu".to_string()),
        "保留席位必须留在名单中"
    );
    for id in &rotated.master_ids() {
        if id != "sun-tzu" {
            assert!(!current.contains(id), "非保留席位应换上新人");
        }
    }
    assert!(
        rotated.layers.len() >= thought_forge_core::council::MIN_LAYER_COVERAGE,
        "换批后层次覆盖约束仍成立"
    );
}

#[test]
fn round_one_prompts_stay_independent_and_skip_corpus() {
    let conn = seeded_db();
    let client = ScriptedClient::new();
    let session = run_session(&conn, &client, Strategy::Steady);

    let calls = client.calls.borrow();
    let round_one: Vec<&ModelRequest> = calls
        .iter()
        .filter(|request| request.purpose == "council_round1")
        .collect();
    assert_eq!(round_one.len(), 6);

    let details: Vec<thought_forge_core::master::MasterDetail> = round_one
        .iter()
        .map(|request| {
            let name = speaker(&request.system);
            let id = masters::list(&conn, None, None)
                .unwrap()
                .into_iter()
                .find(|summary| summary.name == name)
                .expect("大师应存在")
                .id;
            masters::detail(&conn, &id).unwrap()
        })
        .collect();

    for (index, request) in round_one.iter().enumerate() {
        let own: Vec<&str> = details[index]
            .units
            .iter()
            .map(|unit| unit.title.as_str())
            .collect();
        for (other_index, other) in details.iter().enumerate() {
            if other_index == index {
                continue;
            }
            for unit in &other.units {
                assert!(
                    !request.user.contains(&unit.title),
                    "第一轮提示词不得包含其他大师的技能单元：{}",
                    unit.title
                );
            }
        }
        assert!(
            own.iter().any(|title| request.user.contains(title)),
            "第一轮提示词应包含自己的技能单元"
        );
        // property 23：原始语料正文不进入提示词。
        assert!(
            !request.user.contains("作为人，何谓正确"),
            "第一轮不应携带原始语料原文"
        );
    }

    let _ = session;
}

#[test]
fn council_run_records_turns_versions_and_audit() {
    let conn = seeded_db();
    let client = ScriptedClient::new();
    let session = run_session(&conn, &client, Strategy::Steady);

    let detail = repo::session_detail(&conn, &session).expect("可读取会话");
    assert_eq!(detail.session.status, "done");
    assert!(!detail.session.conclusion.is_empty(), "应写入结论");
    assert_eq!(detail.panels.len(), 1);

    let answers: Vec<_> = detail.turns.iter().filter(|turn| turn.round == 1).collect();
    let crosses: Vec<_> = detail.turns.iter().filter(|turn| turn.round == 2).collect();
    let synthesis: Vec<_> = detail.turns.iter().filter(|turn| turn.round == 3).collect();
    assert_eq!(answers.len(), 6);
    assert_eq!(crosses.len(), 6);
    assert_eq!(synthesis.len(), 1);

    // property 21：版本号写入后固定为当时版本。
    for turn in &answers {
        assert_eq!(turn.master_version, Some(1));
        assert_eq!(turn.status, "ok");
        assert!(!turn.content.is_empty());
    }
    assert_eq!(synthesis[0].master_id, None);

    // 调用审计完整：每一次尝试都留痕。
    let calls = thought_forge_core::llm::recent_calls(&conn, 100).unwrap();
    assert_eq!(calls.len(), 13, "六次第一轮、六次第二轮、一次收敛");
    assert!(calls.iter().all(|call| call.status == "ok"));

    let divergences = detail.session.divergences;
    assert!(!divergences.is_empty(), "应标注分歧点");
}

#[test]
fn rotation_adds_a_second_panel_without_losing_history() {
    let conn = seeded_db();
    let pool = build_pool(&conn);
    let session = repo::create_session(
        &conn,
        question(),
        &pool.domains,
        &[],
        Strategy::Steady,
    )
    .unwrap();

    let pinned = vec!["sun-tzu".to_string()];
    let first = select::select_panel(
        &conn,
        &pool,
        &select::SelectionRequest {
            strategy: Strategy::Steady,
            size: 6,
            pinned: &pinned,
            exclude: &[],
        },
    )
    .unwrap();
    repo::record_panel(&conn, &session, 0, &first, &pinned).unwrap();

    let rotated = select::select_panel(
        &conn,
        &pool,
        &select::SelectionRequest {
            strategy: Strategy::Clash,
            size: 6,
            pinned: &pinned,
            exclude: &first.master_ids(),
        },
    )
    .unwrap();
    repo::record_panel(&conn, &session, 1, &rotated, &pinned).unwrap();

    let panels = repo::panels(&conn, &session).unwrap();
    assert_eq!(panels.len(), 2, "两轮阵容都保留");
    assert_eq!(panels[1].rotation, 1);
    assert!(panels[1].pinned_ids.contains(&"sun-tzu".to_string()));
}

#[test]
fn all_failed_calls_mark_session_failed() {
    let conn = seeded_db();
    let client = ScriptedClient::failing();
    let pool = build_pool(&conn);
    let session = repo::create_session(&conn, question(), &pool.domains, &[], Strategy::Steady).unwrap();
    let plan = select::select_panel(&conn, &pool, &select::SelectionRequest::new(Strategy::Steady)).unwrap();
    repo::record_panel(&conn, &session, 0, &plan, &[]).unwrap();

    let policy = RetryPolicy {
        attempts: 2,
        base_delay_ms: 0,
    };
    let result = orchestrator::run_council(&conn, &client, &session, &policy);
    assert!(result.is_err(), "全部失败应报错");

    let detail = repo::session_detail(&conn, &session).unwrap();
    assert_eq!(detail.session.status, "failed");
    assert!(detail
        .turns
        .iter()
        .all(|turn| turn.status == "failed" && turn.error_code.is_some()));

    // 每次尝试都留痕：六席各两次尝试。
    let calls = thought_forge_core::llm::recent_calls(&conn, 100).unwrap();
    assert_eq!(calls.len(), 12);
    assert!(calls.iter().all(|call| call.status == "failed"));
}

fn run_session(
    conn: &rusqlite::Connection,
    client: &dyn ModelClient,
    strategy: Strategy,
) -> String {
    let pool = build_pool(conn);
    let session =
        repo::create_session(conn, question(), &pool.domains, &[], strategy).expect("可新建会话");
    let plan = select::select_panel(conn, &pool, &select::SelectionRequest::new(strategy))
        .expect("可完成选角");
    repo::record_panel(conn, &session, 0, &plan, &[]).expect("可记录阵容");
    let policy = RetryPolicy {
        attempts: 2,
        base_delay_ms: 0,
    };
    orchestrator::run_council(conn, client, &session, &policy).expect("会诊可跑通");
    session
}

/// 分歧明显的客户端：逐次给出互不重叠的措辞，迫使会诊按分歧追加轮次。
struct DivergentClient {
    step: RefCell<usize>,
}

impl DivergentClient {
    fn new() -> Self {
        Self {
            step: RefCell::new(0),
        }
    }
}

const DIVERGENT_WORDS: [&str; 8] = [
    "应当激进扩张，抢占先机，接受波动",
    "务必收缩战线，保住现金，等待时机",
    "优先提升品质，放慢速度，深耕老客",
    "立刻降价促销，换取规模，摊薄成本",
    "坚决维持原价，塑造稀缺，筛选人群",
    "转向免费试用，扩大触点，后端变现",
    "专注单一渠道，做到极致，拒绝分散",
    "并行多路下注，分散风险，快速试错",
];

impl ModelClient for DivergentClient {
    fn complete(&self, _request: &ModelRequest) -> CoreResult<ModelResponse> {
        let index = {
            let mut step = self.step.borrow_mut();
            let current = *step;
            *step += 1;
            current
        };
        Ok(ModelResponse {
            content: DIVERGENT_WORDS[index % DIVERGENT_WORDS.len()].to_string(),
            platform: "divergent".to_string(),
            model: "divergent-1".to_string(),
            prompt_tokens: 10,
            completion_tokens: 20,
        })
    }
}

#[test]
fn high_divergence_appends_rounds_and_writes_the_convergence_curve() {
    use thought_forge_core::council::tuning;

    let conn = seeded_db();
    let client = DivergentClient::new();
    tuning::set(
        &conn,
        &[("council.divergence_threshold".to_string(), "0.1".to_string())],
    )
    .unwrap();

    let session = run_session(&conn, &client, Strategy::Steady);
    let detail = repo::session_detail(&conn, &session).unwrap();

    let cross_rounds: Vec<i64> = detail
        .turns
        .iter()
        .filter(|turn| turn.role == "cross")
        .map(|turn| turn.round)
        .collect();
    assert_eq!(cross_rounds, vec![2; 6].into_iter().chain(vec![3; 6]).collect::<Vec<_>>());

    assert_eq!(detail.metrics.len(), 2, "两个质询轮各一条指标");
    assert_eq!(detail.metrics[0].round, 2);
    assert_eq!(detail.metrics[1].round, 3);
    for metric in &detail.metrics {
        assert_eq!(metric.participant_count, 6);
        assert!(metric.divergence > 0.1);
    }

    let synthesis: Vec<_> = detail.turns.iter().filter(|turn| turn.role == "synthesis").collect();
    assert_eq!(synthesis.len(), 1);
    assert_eq!(synthesis[0].round, 4, "收敛裁决单独计一轮");

    let calls = thought_forge_core::llm::recent_calls(&conn, 100).unwrap();
    assert_eq!(calls.len(), 19, "六次作答、两轮各六次质询、一次收敛");
}

#[test]
fn max_rounds_caps_the_discussion_even_when_divergence_stays_high() {
    use thought_forge_core::council::tuning;

    let conn = seeded_db();
    let client = DivergentClient::new();
    tuning::set(
        &conn,
        &[
            ("council.max_rounds".to_string(), "2".to_string()),
            ("council.divergence_threshold".to_string(), "0.1".to_string()),
        ],
    )
    .unwrap();

    let session = run_session(&conn, &client, Strategy::Steady);
    let detail = repo::session_detail(&conn, &session).unwrap();
    assert_eq!(detail.metrics.len(), 1);
    let crosses = detail.turns.iter().filter(|turn| turn.role == "cross").count();
    assert_eq!(crosses, 6, "达到上限后不再追加");
    let synthesis = detail
        .turns
        .iter()
        .find(|turn| turn.role == "synthesis")
        .unwrap();
    assert_eq!(synthesis.round, 3);
}

/// 首次调用失败、之后成功的客户端，用于制造单席失败与重试场景。
struct FlakyClient {
    calls: RefCell<usize>,
}

impl FlakyClient {
    fn new() -> Self {
        Self {
            calls: RefCell::new(0),
        }
    }
}

impl ModelClient for FlakyClient {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse> {
        let mut calls = self.calls.borrow_mut();
        *calls += 1;
        if *calls == 1 {
            return Err(CoreError::ModelUnavailable {
                status: 500,
                message: "席位首次调用失败".to_string(),
            });
        }
        let name = speaker(&request.system);
        Ok(ModelResponse {
            content: format!("{name}的判断：先看动机与边界，再谈取舍；需要结合时机判断。"),
            platform: "flaky".to_string(),
            model: "flaky-1".to_string(),
            prompt_tokens: 10,
            completion_tokens: 20,
        })
    }
}

fn record_panel_for(
    conn: &rusqlite::Connection,
    strategy: Strategy,
) -> String {
    let pool = build_pool(conn);
    let session =
        repo::create_session(conn, question(), &pool.domains, &[], strategy).expect("可新建会话");
    let plan = select::select_panel(conn, &pool, &select::SelectionRequest::new(strategy))
        .expect("可完成选角");
    repo::record_panel(conn, &session, 0, &plan, &[]).expect("可记录阵容");
    session
}

#[test]
fn seat_speech_groups_turns_and_synthesizes_status() {
    let conn = seeded_db();
    let client = ScriptedClient::new();
    let session = run_session(&conn, &client, Strategy::Steady);

    let seats = speech::seat_speech(&conn, &session, 0).expect("可读逐席发言");
    assert_eq!(seats.len(), 6, "六席各一条");
    for seat in &seats {
        assert_eq!(seat.status, "answered", "脚本化客户端下全部成功");
        assert!(!seat.master_name.is_empty());
        assert!(seat
            .rounds
            .iter()
            .any(|round| round.round == 1 && round.role == "answer"));
        assert!(seat
            .rounds
            .iter()
            .any(|round| round.round == 2 && round.role == "cross"));
    }
}

#[test]
fn seat_speech_marks_failed_seats_with_error_codes() {
    let conn = seeded_db();
    let session = record_panel_for(&conn, Strategy::Steady);
    let client = ScriptedClient::failing();
    let policy = RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    };
    assert!(
        orchestrator::run_council(&conn, &client, &session, &policy).is_err(),
        "全员失败时整场失败"
    );

    let seats = speech::seat_speech(&conn, &session, 0).expect("可读逐席发言");
    assert_eq!(seats.len(), 6);
    for seat in &seats {
        assert_eq!(seat.status, "failed");
        assert!(seat
            .rounds
            .iter()
            .all(|round| round.status == "failed" && round.error_code.is_some()));
    }
}

#[test]
fn failed_seat_can_be_retried_without_rerunning_the_session() {
    let conn = seeded_db();
    let session = record_panel_for(&conn, Strategy::Steady);
    let policy = RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    };
    orchestrator::run_council(&conn, &FlakyClient::new(), &session, &policy)
        .expect("其余席位成功，会诊照常完成");

    let seats = speech::seat_speech(&conn, &session, 0).expect("可读逐席发言");
    let failed = seats
        .iter()
        .find(|seat| seat.status == "failed")
        .expect("应有一个失败席位")
        .master_id
        .clone();
    let before_turns = repo::turns(&conn, &session, Some(0)).expect("可读发言").len();

    let retried = speech::retry_seat(
        &conn,
        &ScriptedClient::new(),
        &session,
        0,
        &failed,
        1,
        &policy,
    )
    .expect("可重试失败席位");

    let seat = retried
        .iter()
        .find(|seat| seat.master_id == failed)
        .expect("重试后仍能看到该席位");
    assert_eq!(seat.status, "answered", "重试成功后席位状态转为已作答");
    assert_eq!(
        repo::turns(&conn, &session, Some(0)).expect("可读发言").len(),
        before_turns + 1,
        "只追加一条轮次记录"
    );
}

/// 脚本化检索：记录调用次数与问句，返回固定条数的外部资料。
struct ScriptedSearch {
    calls: RefCell<usize>,
    queries: RefCell<Vec<String>>,
}

impl ScriptedSearch {
    fn new() -> Self {
        Self {
            calls: RefCell::new(0),
            queries: RefCell::new(Vec::new()),
        }
    }

    fn call_count(&self) -> usize {
        *self.calls.borrow()
    }
}

impl SearchProvider for ScriptedSearch {
    fn search(&self, query: &str, limit: usize) -> CoreResult<Vec<SearchHit>> {
        *self.calls.borrow_mut() += 1;
        self.queries.borrow_mut().push(query.to_string());
        Ok((0..limit.min(3))
            .map(|index| SearchHit {
                title: format!("外部资料{index}"),
                url: format!("https://example.com/{index}"),
                snippet: format!("这是第 {index} 条外部资料的内容"),
                published_at: Some("2026-06-01T00:00:00Z".to_string()),
            })
            .collect())
    }
}

fn run_session_with_retrieval(
    conn: &rusqlite::Connection,
    client: &dyn ModelClient,
    retrieval: &Retrieval<'_>,
) -> String {
    let pool = build_pool(conn);
    let session = repo::create_session(conn, question(), &pool.domains, &[], Strategy::Steady)
        .expect("可新建会话");
    let plan = select::select_panel(conn, &pool, &select::SelectionRequest::new(Strategy::Steady))
        .expect("可完成选角");
    repo::record_panel(conn, &session, 0, &plan, &[]).expect("可记录阵容");
    let policy = RetryPolicy {
        attempts: 2,
        base_delay_ms: 0,
    };
    orchestrator::run_council_with_retrieval(conn, client, retrieval, &session, &policy)
        .expect("会诊可跑通");
    session
}

#[test]
fn shared_background_reaches_every_seat_and_is_frozen() {
    use thought_forge_core::connector::service as connector_service;

    let conn = seeded_db();
    let client = ScriptedClient::new();
    let search = ScriptedSearch::new();
    let retrieval = Retrieval {
        search: Some(&search),
        page: None,
    };
    let session = run_session_with_retrieval(&conn, &client, &retrieval);

    let seat_prompts = requests_of(&client, "council_round1");
    assert_eq!(seat_prompts.len(), 6);
    assert!(
        seat_prompts
            .iter()
            .all(|request| request.user.contains("共享背景")),
        "共享背景应注入全部席位"
    );
    assert!(seat_prompts
        .iter()
        .all(|request| request.user.contains("[1] 外部资料0")));
    assert_eq!(search.call_count(), 1, "共享背景只检索一次");

    let sources = connector_service::sources(&conn, &session, 0).expect("可读快照");
    assert_eq!(sources.len(), 3);
    assert!(sources.iter().all(|source| source.master_id.is_none()));
}

#[test]
fn seat_search_only_reaches_its_own_prompt() {
    use thought_forge_core::connector::service as connector_service;
    use thought_forge_core::council::tuning;

    let conn = seeded_db();
    tuning::set(
        &conn,
        &[
            ("council.shared_background".to_string(), "false".to_string()),
            ("council.seat_search".to_string(), "true".to_string()),
            (
                "connector.max_searches_per_session".to_string(),
                "20".to_string(),
            ),
        ],
    )
    .unwrap();
    let client = ScriptedClient::new();
    let search = ScriptedSearch::new();
    let retrieval = Retrieval {
        search: Some(&search),
        page: None,
    };
    let session = run_session_with_retrieval(&conn, &client, &retrieval);

    assert_eq!(search.call_count(), 6, "每个席位各补一次检索");
    for query in search.queries.borrow().iter() {
        assert!(!query.is_empty(), "席位检索有可发送的内容");
        assert!(
            !query.contains(question()),
            "关键词模式下不整句外发议题原文"
        );
    }
    // 审计里同时留下原问句与真正发出的串，便于逐字核对。
    let calls = thought_forge_core::connector::repo::recent_calls(&conn, 20).expect("可读审计");
    let seat_calls: Vec<_> = calls
        .iter()
        .filter(|call| call.purpose == "council_seat_search")
        .collect();
    assert_eq!(seat_calls.len(), 6);
    assert!(
        seat_calls
            .iter()
            .all(|call| call.query_original.starts_with(question())),
        "席位检索的原始问句以议题为起点"
    );
    assert!(seat_calls.iter().all(|call| !call.query_sent.is_empty()));
    let seat_prompts = requests_of(&client, "council_round1");
    assert_eq!(seat_prompts.len(), 6);
    assert!(
        seat_prompts
            .iter()
            .all(|request| request.user.contains("该席位补充检索")),
        "席位补充检索只标注在本席提示词里"
    );

    let panels = repo::panels(&conn, &session).expect("可读阵容");
    for master_id in &panels[0].master_ids {
        let owned =
            connector_service::seat_sources(&conn, &session, 0, master_id).expect("可读席位快照");
        assert!(!owned.is_empty(), "该席位应有自己的补充检索快照");
        assert!(owned
            .iter()
            .all(|source| source.master_id.as_deref() == Some(master_id.as_str())));
    }
}

fn requests_of(client: &ScriptedClient, purpose: &str) -> Vec<ModelRequest> {
    client
        .calls
        .borrow()
        .iter()
        .filter(|request| request.purpose == purpose)
        .cloned()
        .collect()
}

/// 脚本化极性判定：恒定回一个结论，并记录被调用次数。
struct ScriptedJudge {
    calls: RefCell<usize>,
    verdict: bool,
}

impl ScriptedJudge {
    fn new(verdict: bool) -> Self {
        Self {
            calls: RefCell::new(0),
            verdict,
        }
    }

    fn call_count(&self) -> usize {
        *self.calls.borrow()
    }
}

impl thought_forge_core::council::divergence::PolarityJudge for ScriptedJudge {
    fn opposing(&self, _left: &str, _right: &str) -> CoreResult<bool> {
        *self.calls.borrow_mut() += 1;
        Ok(self.verdict)
    }
}

/// 脚本化客户端始终给出同一段话，词面重合度极高，正是极性判定要修正的场景。
fn run_session_with_judge(
    conn: &rusqlite::Connection,
    client: &dyn ModelClient,
    judge: &dyn thought_forge_core::council::divergence::PolarityJudge,
) -> String {
    let pool = build_pool(conn);
    let session = repo::create_session(conn, question(), &pool.domains, &[], Strategy::Steady)
        .expect("可新建会话");
    let plan = select::select_panel(conn, &pool, &select::SelectionRequest::new(Strategy::Steady))
        .expect("可完成选角");
    repo::record_panel(conn, &session, 0, &plan, &[]).expect("可记录阵容");
    let policy = RetryPolicy {
        attempts: 2,
        base_delay_ms: 0,
    };
    orchestrator::run_council_with_judge(
        conn,
        client,
        &Retrieval::none(),
        &session,
        &policy,
        Some(judge),
    )
    .expect("会诊可跑通");
    session
}

#[test]
fn hybrid_judgement_is_recorded_and_prompt_version_is_traceable() {
    use thought_forge_core::council::tuning;

    let conn = seeded_db();
    // 把判定对数压到一，验证上限与「判定方式入档」两件事。
    tuning::set(
        &conn,
        &[
            ("council.polarity_max_pairs".to_string(), "1".to_string()),
            ("council.max_rounds".to_string(), "2".to_string()),
        ],
    )
    .expect("可设调参");
    let client = ScriptedClient::new();
    let judge = ScriptedJudge::new(true);
    let session = run_session_with_judge(&conn, &client, &judge);

    assert_eq!(judge.call_count(), 1, "对数上限生效");

    let metrics = repo::metrics(&conn, &session, 0).expect("可读曲线");
    assert!(!metrics.is_empty(), "第一轮指标应落库");
    let first = &metrics[0];
    assert_eq!(first.method, "hybrid", "判定方式入档");
    assert!(!first.fell_back, "判定可用时不应标回退");
    assert!(
        first.avg_similarity < 0.9,
        "判定为对立的对按零计入，均值应明显下降"
    );

    let turns = repo::turns(&conn, &session, Some(0)).expect("可读发言");
    assert!(!turns.is_empty());
    assert!(
        turns
            .iter()
            .all(|turn| turn.prompt_version == orchestrator::PROMPT_VERSION),
        "每条发言都要留下提示词版本"
    );
}

#[test]
fn judge_free_run_still_records_lexical_method() {
    let conn = seeded_db();
    let client = ScriptedClient::new();
    // 未注入判定能力时，混合模式必须回退词面并留痕，而不是整场失败。
    let session = run_session_with_retrieval(&conn, &client, &Retrieval::none());

    let metrics = repo::metrics(&conn, &session, 0).expect("可读曲线");
    assert!(!metrics.is_empty());
    assert_eq!(metrics[0].method, "lexical");
    assert!(metrics[0].fell_back, "回退要标出来");
}

/// 造一个带单价的平台并记一笔费用，让下一次估算必然越过上限。
fn arm_cost_limit(conn: &rusqlite::Connection, limit: &str, policy: &str) {
    let platform = thought_forge_core::llm::platform::upsert(
        conn,
        &thought_forge_core::llm::platform::PlatformInput {
            code: "cloud".to_string(),
            display_name: "云端平台".to_string(),
            endpoint: "https://api.example.com/v1/chat/completions".to_string(),
            model_name: "gpt-x".to_string(),
            input_price_micros_per_1k: 2_000,
            output_price_micros_per_1k: 6_000,
            currency: "CNY".to_string(),
        },
    )
    .unwrap();
    thought_forge_core::llm::platform::set_enabled(conn, &platform.code, true).unwrap();
    thought_forge_core::cost::record_llm_cost(conn, 900, 300, &platform.code).unwrap();
    thought_forge_core::council::tuning::set(
        conn,
        &[
            ("cost.daily_limit_micros".to_string(), limit.to_string()),
            ("cost.over_limit_policy".to_string(), policy.to_string()),
        ],
    )
    .unwrap();
}

#[test]
fn quota_gate_rejects_when_over_limit() {
    let conn = seeded_db();
    arm_cost_limit(&conn, "1000", thought_forge_core::cost::POLICY_REJECT);

    let client = ScriptedClient::new();
    let pool = build_pool(&conn);
    let session = repo::create_session(&conn, question(), &pool.domains, &[], Strategy::Steady).unwrap();
    let plan = select::select_panel(&conn, &pool, &select::SelectionRequest::new(Strategy::Steady)).unwrap();
    repo::record_panel(&conn, &session, 0, &plan, &[]).unwrap();

    let policy = RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    };
    let error = orchestrator::run_council(&conn, &client, &session, &policy).unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
    assert!(client.calls.borrow().is_empty(), "配额拒绝时不应发生模型调用");
}

#[test]
fn quota_gate_records_compressed_scope() {
    let conn = seeded_db();
    arm_cost_limit(&conn, "1000", thought_forge_core::cost::POLICY_REDUCE_ROUNDS);

    let client = ScriptedClient::new();
    let session = run_session(&conn, &client, Strategy::Steady);
    let view = repo::get_session(&conn, &session).unwrap();
    assert_eq!(
        view.quota_max_rounds,
        Some(thought_forge_core::cost::REDUCED_ROUNDS)
    );
    assert_eq!(view.quota_max_seats, None);
    assert!(view.quota_reason.contains("上限"));
    // 压缩后的轮次上限生效：最多一次交叉质询轮，加一次收敛裁决。
    let detail = repo::session_detail(&conn, &session).unwrap();
    let cross_rounds: std::collections::BTreeSet<i64> = detail
        .turns
        .iter()
        .filter(|turn| turn.role == "cross")
        .map(|turn| turn.round)
        .collect();
    assert!(cross_rounds.len() <= 1, "压缩后不应出现更多质询轮");
}

#[test]
fn conclusion_view_reports_calls_and_cost_estimate() {
    let conn = seeded_db();
    let platform = thought_forge_core::llm::platform::upsert(
        &conn,
        &thought_forge_core::llm::platform::PlatformInput {
            code: "cloud".to_string(),
            display_name: "云端平台".to_string(),
            endpoint: "https://api.example.com/v1/chat/completions".to_string(),
            model_name: "gpt-x".to_string(),
            input_price_micros_per_1k: 2_000,
            output_price_micros_per_1k: 6_000,
            currency: "CNY".to_string(),
        },
    )
    .unwrap();
    thought_forge_core::llm::platform::set_enabled(&conn, &platform.code, true).unwrap();

    let client = ScriptedClient::new();
    let session = run_session(&conn, &client, Strategy::Steady);
    let view =
        thought_forge_core::council::conclusion::conclusion_view(&conn, &session).unwrap();

    assert!(view.llm_calls > 0, "结论页应带上本场模型调用次数");
    assert_eq!(view.search_calls, 0, "未启用检索时检索次数为零");
    assert!(view.priced, "已配置单价时不该标为零计");
    assert!(view.cost_micros > 0, "费用估算应反映平台单价");
    assert_eq!(view.currency, "CNY");
}

#[test]
fn cancelled_session_stops_before_any_call() {
    use thought_forge_core::council::control;

    let conn = seeded_db();
    let client = ScriptedClient::new();
    let pool = build_pool(&conn);
    let session = repo::create_session(&conn, question(), &pool.domains, &[], Strategy::Steady)
        .expect("可新建会话");
    let plan = select::select_panel(&conn, &pool, &select::SelectionRequest::new(Strategy::Steady))
        .expect("可完成选角");
    repo::record_panel(&conn, &session, 0, &plan, &[]).expect("可记录阵容");
    control::request_cancel(&conn, &session).expect("可请求取消");

    let policy = RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    };
    let outcome =
        orchestrator::run_council(&conn, &client, &session, &policy).expect("取消后仍返回结论对象");
    assert!(client.calls.borrow().is_empty(), "取消后不再发起模型调用");
    assert_eq!(outcome.rounds, 0);

    let view = repo::get_session(&conn, &session).unwrap();
    assert_eq!(view.status, "cancelled");
    assert!(view.cancelled_at.is_some(), "取消要留时刻");
}

#[test]
fn cancel_preserves_turns_already_written() {
    use thought_forge_core::council::control;

    let conn = seeded_db();
    let client = ScriptedClient::new();
    let session = run_session(&conn, &client, Strategy::Steady);
    let turns_before = repo::turns(&conn, &session, None).unwrap().len();
    let calls_before = client.calls.borrow().len();
    assert!(turns_before > 0, "首次会诊已写入逐席发言");

    // 恢复后再次请求取消：已完成轮次保留，不再新增调用。
    repo::update_status(&conn, &session, "running").unwrap();
    control::request_cancel(&conn, &session).expect("可请求取消");
    let policy = RetryPolicy {
        attempts: 1,
        base_delay_ms: 0,
    };
    orchestrator::run_council(&conn, &client, &session, &policy).expect("取消后仍返回结论对象");

    assert_eq!(
        client.calls.borrow().len(),
        calls_before,
        "取消后不再新增模型调用"
    );
    assert_eq!(
        repo::turns(&conn, &session, None).unwrap().len(),
        turns_before,
        "已完成轮次全部保留"
    );
    let view = repo::get_session(&conn, &session).unwrap();
    assert_eq!(view.status, "cancelled");
    assert!(view.cancelled_at.is_some(), "取消要留时刻");
}

#[test]
fn resume_reuses_completed_rounds_and_synthesis() {
    use thought_forge_core::council::control;

    let conn = seeded_db();
    let client = ScriptedClient::new();
    let session = run_session(&conn, &client, Strategy::Steady);
    let first_calls = client.calls.borrow().len();
    assert!(first_calls > 0, "首次会诊应有模型调用");

    // 模拟应用在一次会诊未完成时退出：状态回到运行中且心跳过期。
    repo::update_status(&conn, &session, "running").unwrap();
    conn.execute(
        "UPDATE council_sessions
         SET heartbeat_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-1 hour')
         WHERE id = ?1",
        [&session],
    )
    .unwrap();
    let recoverable = control::recoverable(&conn, 120).unwrap();
    assert!(
        recoverable.iter().any(|view| view.id == session),
        "中断会话应出现在恢复列表"
    );

    let turns_before = repo::turns(&conn, &session, None).unwrap().len();
    let policy = RetryPolicy {
        attempts: 2,
        base_delay_ms: 0,
    };
    orchestrator::run_council(&conn, &client, &session, &policy).expect("可断点续跑");

    assert_eq!(
        client.calls.borrow().len(),
        first_calls,
        "续跑不重发已完成发言，也不重复收敛裁决"
    );
    assert_eq!(
        repo::turns(&conn, &session, None).unwrap().len(),
        turns_before,
        "续跑不新增重复发言"
    );
    assert_eq!(repo::get_session(&conn, &session).unwrap().status, "done");
}
