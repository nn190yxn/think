//! 分歧判定：词面判定、极性优先、对数上限与回退留痕。

use std::cell::RefCell;

use thought_forge_core::council::divergence::{self, DivergenceMode, PolarityJudge};
use thought_forge_core::council::tuning;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::{ModelClient, ModelRequest, ModelResponse};
use thought_forge_core::{CoreError, CoreResult};

/// 措辞几乎全同但结论相反的一对发言：词面重合度高，极性判定应能翻过来。
const AGREE_SIDE: (&str, &str) = ("甲", "应该提高定价");
const OPPOSITE_SIDE: (&str, &str) = ("乙", "不应该提高定价");

struct ScriptedJudge {
    calls: RefCell<usize>,
    verdict: CoreResult<bool>,
}

impl ScriptedJudge {
    fn new(verdict: CoreResult<bool>) -> Self {
        Self {
            calls: RefCell::new(0),
            verdict,
        }
    }

    fn call_count(&self) -> usize {
        *self.calls.borrow()
    }
}

impl PolarityJudge for ScriptedJudge {
    fn opposing(&self, _left: &str, _right: &str) -> CoreResult<bool> {
        *self.calls.borrow_mut() += 1;
        match &self.verdict {
            Ok(value) => Ok(*value),
            Err(error) => Err(CoreError::ModelUnavailable {
                status: 500,
                message: error.to_string(),
            }),
        }
    }
}

fn memory_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn set_tuning(conn: &rusqlite::Connection, key: &str, value: &str) {
    tuning::set(conn, &[(key.to_string(), value.to_string())]).expect("调参可写入");
}

fn answers(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(who, text)| (who.to_string(), text.to_string()))
        .collect()
}

#[test]
fn lexical_mode_never_calls_the_judge() {
    let conn = memory_db();
    let judge = ScriptedJudge::new(Ok(true));
    let stats = divergence::round_metric(
        &conn,
        Some(&judge),
        DivergenceMode::Lexical,
        &answers(&[AGREE_SIDE, OPPOSITE_SIDE]),
    )
    .expect("词面判定可算");

    assert_eq!(judge.call_count(), 0, "词面模式不消耗极性判定");
    assert_eq!(stats.method, "lexical");
    assert!(!stats.fell_back);
    assert!((stats.avg_similarity - 1.0).abs() < 1e-9, "措辞全同测得高重合");
    assert!(stats.divergence.abs() < 1e-9);
}

#[test]
fn polarity_flips_high_overlap_agreement() {
    let conn = memory_db();
    let judge = ScriptedJudge::new(Ok(true));
    let stats = divergence::round_metric(
        &conn,
        Some(&judge),
        DivergenceMode::Hybrid,
        &answers(&[AGREE_SIDE, OPPOSITE_SIDE]),
    )
    .expect("混合判定可算");

    assert_eq!(judge.call_count(), 1, "只有一对候选，判定一次");
    assert_eq!(stats.method, "hybrid");
    assert!(!stats.fell_back);
    assert!(
        stats.avg_similarity.abs() < 1e-9,
        "判定为对立的对按零计入均值"
    );
    assert!((stats.divergence - 1.0).abs() < 1e-9);
}

#[test]
fn low_overlap_pairs_skip_polarity_entirely() {
    let conn = memory_db();
    let judge = ScriptedJudge::new(Ok(true));
    let stats = divergence::round_metric(
        &conn,
        Some(&judge),
        DivergenceMode::Hybrid,
        &answers(&[("甲", "应当激进扩张抢占先机"), ("乙", "务必收缩战线保住现金")]),
    )
    .expect("混合判定可算");

    assert_eq!(judge.call_count(), 0, "重合度不足的对不做极性判定");
    assert_eq!(stats.method, "hybrid");
    assert!(!stats.fell_back, "没有候选对不算回退");
    assert!(stats.avg_similarity.abs() < 1e-9);
}

#[test]
fn judge_calls_are_bounded_by_the_pair_limit() {
    let conn = memory_db();
    set_tuning(&conn, "council.polarity_max_pairs", "1");
    let judge = ScriptedJudge::new(Ok(true));
    let stats = divergence::round_metric(
        &conn,
        Some(&judge),
        DivergenceMode::Hybrid,
        &answers(&[AGREE_SIDE, ("丙", "应该提高价格"), ("丁", "应该提高定价"), ("戊", "应该提高定价")]),
    )
    .expect("混合判定可算");

    assert_eq!(judge.call_count(), 1, "对数上限生效");
    // 六对里有五对保持原值、一对按零计。
    assert!(stats.avg_similarity < 1.0 && stats.avg_similarity > 0.5);
    assert_eq!(stats.method, "hybrid");
}

#[test]
fn missing_judge_falls_back_to_lexical_and_marks_it() {
    let conn = memory_db();
    let stats = divergence::round_metric(
        &conn,
        None,
        DivergenceMode::Hybrid,
        &answers(&[AGREE_SIDE, OPPOSITE_SIDE]),
    )
    .expect("无判定能力时仍可算");

    assert_eq!(stats.method, "lexical", "判定方式降级为词面");
    assert!(stats.fell_back, "回退要留痕");
    assert!((stats.avg_similarity - 1.0).abs() < 1e-9, "回退后沿用词面结果");
}

#[test]
fn failing_judge_degrades_instead_of_aborting() {
    let conn = memory_db();
    let judge = ScriptedJudge::new(Err(CoreError::ModelUnavailable {
        status: 503,
        message: "脚本化失败".to_string(),
    }));
    let stats = divergence::round_metric(
        &conn,
        Some(&judge),
        DivergenceMode::Hybrid,
        &answers(&[AGREE_SIDE, OPPOSITE_SIDE]),
    )
    .expect("判定失败不中断会诊");

    assert_eq!(judge.call_count(), 1);
    assert_eq!(stats.method, "lexical");
    assert!(stats.fell_back);
}

#[test]
fn single_participant_yields_zero_divergence() {
    let conn = memory_db();
    let judge = ScriptedJudge::new(Ok(true));
    let stats = divergence::round_metric(
        &conn,
        Some(&judge),
        DivergenceMode::Hybrid,
        &answers(&[AGREE_SIDE]),
    )
    .expect("单人轮次可算");

    assert_eq!(stats.participant_count, 1);
    assert!(stats.divergence.abs() < 1e-9, "不足两人不触发追加");
    assert_eq!(judge.call_count(), 0);
}

#[test]
fn verdict_parsing_accepts_only_two_outcomes() {
    use divergence::ModelPolarityJudge;

    assert_eq!(ModelPolarityJudge::parse_verdict("对立\n理由略"), Some(true));
    assert_eq!(ModelPolarityJudge::parse_verdict("相反。"), Some(true));
    assert_eq!(ModelPolarityJudge::parse_verdict("一致"), Some(false));
    assert_eq!(ModelPolarityJudge::parse_verdict("不对立，方向相同"), Some(false));
    assert_eq!(ModelPolarityJudge::parse_verdict("  \n 冲突"), Some(true));
    assert_eq!(ModelPolarityJudge::parse_verdict(""), None);
    assert_eq!(ModelPolarityJudge::parse_verdict("很难判断，各有道理"), None);
}

/// 只回答一句话的客户端，用于验证极性判定确实走了模型并要求版本号。
struct OneLinerClient;

impl ModelClient for OneLinerClient {
    fn complete(&self, request: &ModelRequest) -> CoreResult<ModelResponse> {
        assert!(
            request.user.contains("甲：") && request.user.contains("乙："),
            "极性判定的提示词应带上两段发言"
        );
        Ok(ModelResponse {
            content: "对立".to_string(),
            platform: "scripted".to_string(),
            model: "scripted-1".to_string(),
            prompt_tokens: 5,
            completion_tokens: 5,
        })
    }
}

#[test]
fn model_judge_reads_the_verdict_and_records_the_prompt_version() {
    let conn = memory_db();
    let client = OneLinerClient;
    let judge = divergence::ModelPolarityJudge {
        conn: &conn,
        client: &client,
        policy: thought_forge_core::llm::RetryPolicy {
            attempts: 1,
            base_delay_ms: 0,
        },
    };
    assert!(judge.opposing("应该提高定价", "不应该提高定价").expect("判定可完成"));

    let calls = thought_forge_core::llm::recent_calls(&conn, 10).expect("可读审计");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].purpose, divergence::POLARITY_PURPOSE);
    assert_eq!(calls[0].status, "ok");

    let recorded: String = conn
        .query_row("SELECT prompt_version FROM llm_calls LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("可读版本号");
    assert_eq!(
        recorded,
        thought_forge_core::council::orchestrator::PROMPT_VERSION,
        "每次模型调用都要留下提示词版本"
    );
}

#[test]
fn unparsable_verdict_is_an_error_so_the_caller_can_fall_back() {
    struct Mumbler;
    impl ModelClient for Mumbler {
        fn complete(&self, _request: &ModelRequest) -> CoreResult<ModelResponse> {
            Ok(ModelResponse {
                content: "这个要看具体情况".to_string(),
                platform: "scripted".to_string(),
                model: "scripted-1".to_string(),
                prompt_tokens: 1,
                completion_tokens: 1,
            })
        }
    }

    let conn = memory_db();
    let judge = divergence::ModelPolarityJudge {
        conn: &conn,
        client: &Mumbler,
        policy: thought_forge_core::llm::RetryPolicy {
            attempts: 1,
            base_delay_ms: 0,
        },
    };
    let error = judge.opposing("甲说", "乙说").expect_err("无法解析应报错");
    assert_eq!(error.code(), "E_MALFORMED_RESPONSE");

    // 落到轮次指标上表现为回退，而不是整场会诊失败。
    let stats = divergence::round_metric(
        &conn,
        Some(&judge),
        DivergenceMode::Hybrid,
        &answers(&[AGREE_SIDE, OPPOSITE_SIDE]),
    )
    .expect("解析失败仍可算");
    assert!(stats.fell_back);
    assert_eq!(stats.method, "lexical");
}
