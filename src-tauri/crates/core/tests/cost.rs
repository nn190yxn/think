//! 成本治理：估算公式、整数微元记账、日与月汇总与三种超限策略。

use rusqlite::Connection;
use thought_forge_core::connector::{repo as connector_repo, ConnectorInput};
use thought_forge_core::cost;
use thought_forge_core::council::tuning;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::platform::{self as platform_repo, PlatformInput};

fn db() -> Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn add_platform(conn: &Connection, code: &str, input_price: i64, output_price: i64) {
    platform_repo::upsert(
        conn,
        &PlatformInput {
            code: code.to_string(),
            display_name: code.to_string(),
            endpoint: "https://api.example.com/v1/chat/completions".to_string(),
            model_name: "model-x".to_string(),
            input_price_micros_per_1k: input_price,
            output_price_micros_per_1k: output_price,
            currency: "CNY".to_string(),
        },
    )
    .expect("平台可写入");
}

fn set_limit(conn: &Connection, key: &str, value: &str) {
    tuning::set(conn, &[(key.to_string(), value.to_string())]).expect("调参可写入");
}

#[test]
fn estimate_formula_matches_seats_rounds_and_searches() {
    let conn = db();
    add_platform(&conn, "cloud", 2_000, 6_000);
    platform_repo::set_enabled(&conn, "cloud", true).unwrap();

    let estimate = cost::estimate(&conn, 6, 3, 7).unwrap();
    // 每席每轮一次调用，另加一次收敛裁决。
    assert_eq!(estimate.llm_calls, 6 * 3 + 1);
    assert_eq!(estimate.search_calls, 7);
    assert_eq!(
        estimate.tokens,
        estimate.llm_calls * (cost::ASSUMED_PROMPT_TOKENS_PER_CALL + cost::ASSUMED_COMPLETION_TOKENS_PER_CALL)
    );
    // 19 次调用 ×（900 × 2000 + 300 × 6000）/ 1000 = 19 × 3600 微元。
    assert_eq!(estimate.cost_micros, 19 * 3_600);
    assert_eq!(estimate.platform_code, "cloud");
    assert!(estimate.priced);
}

#[test]
fn unpriced_platform_counts_zero_and_marks_not_priced() {
    let conn = db();
    add_platform(&conn, "local", 0, 0);
    platform_repo::set_enabled(&conn, "local", true).unwrap();

    let estimate = cost::estimate(&conn, 6, 3, 7).unwrap();
    assert_eq!(estimate.cost_micros, 0);
    assert!(!estimate.priced);
    assert_eq!(cost::record_llm_cost(&conn, 900, 300, "local").unwrap(), 0);
    assert_eq!(cost::today_and_month_micros(&conn).unwrap().0, 0);
}

#[test]
fn llm_and_connector_costs_accumulate_by_day() {
    let conn = db();
    add_platform(&conn, "cloud", 2_000, 6_000);
    platform_repo::set_enabled(&conn, "cloud", true).unwrap();
    let connector = connector_repo::upsert(
        &conn,
        &ConnectorInput {
            id: None,
            kind: "search".to_string(),
            display_name: "演示检索".to_string(),
            endpoint: "https://search.example.com".to_string(),
            config: serde_json::json!({ "costMicrosPerCall": 2_500 }),
        },
    )
    .unwrap();

    let llm = cost::record_llm_cost(&conn, 900, 300, "cloud").unwrap();
    let search = cost::record_connector_cost(&conn, Some(&connector.id)).unwrap();
    assert_eq!(llm, 3_600);
    assert_eq!(search, 2_500);

    let summary = cost::cost_summary(&conn, 7).unwrap();
    assert_eq!(summary.days.len(), 1);
    assert_eq!(summary.days[0].calls, 2);
    assert_eq!(summary.days[0].tokens, 1_200);
    assert_eq!(summary.days[0].cost_micros, 6_100);
    assert_eq!(summary.today_micros, 6_100);
    assert_eq!(summary.month_micros, 6_100);
    assert_eq!(summary.currency, "CNY");
    assert!(summary.priced);
}

#[test]
fn guard_allows_when_within_limit() {
    let conn = db();
    add_platform(&conn, "cloud", 2_000, 6_000);
    platform_repo::set_enabled(&conn, "cloud", true).unwrap();
    set_limit(&conn, "cost.daily_limit_micros", "100000");

    let estimate = cost::estimate(&conn, 6, 3, 7).unwrap();
    let decision = cost::guard(&conn, &estimate).unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.max_rounds, None);
    assert_eq!(decision.max_seats, None);
}

#[test]
fn guard_rejects_when_over_daily_limit() {
    let conn = db();
    add_platform(&conn, "cloud", 2_000, 6_000);
    platform_repo::set_enabled(&conn, "cloud", true).unwrap();
    // 先记一笔接近上限的费用，让本次估算必然越界。
    cost::record_llm_cost(&conn, 900, 300, "cloud").unwrap();
    set_limit(&conn, "cost.daily_limit_micros", "5000");

    let estimate = cost::estimate(&conn, 6, 3, 7).unwrap();
    let decision = cost::guard(&conn, &estimate).unwrap();
    assert!(!decision.allowed);
    assert_eq!(decision.policy, cost::POLICY_REJECT);
    assert!(decision.reason.contains("上限"));
}

#[test]
fn guard_reduce_rounds_returns_compressed_rounds() {
    let conn = db();
    add_platform(&conn, "cloud", 2_000, 6_000);
    platform_repo::set_enabled(&conn, "cloud", true).unwrap();
    cost::record_llm_cost(&conn, 900, 300, "cloud").unwrap();
    set_limit(&conn, "cost.daily_limit_micros", "5000");
    set_limit(&conn, "cost.over_limit_policy", cost::POLICY_REDUCE_ROUNDS);

    let estimate = cost::estimate(&conn, 6, 4, 7).unwrap();
    let decision = cost::guard(&conn, &estimate).unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.max_rounds, Some(cost::REDUCED_ROUNDS));
    assert_eq!(decision.max_seats, None);
}

#[test]
fn guard_reduce_seats_returns_compressed_seats() {
    let conn = db();
    add_platform(&conn, "cloud", 2_000, 6_000);
    platform_repo::set_enabled(&conn, "cloud", true).unwrap();
    cost::record_llm_cost(&conn, 900, 300, "cloud").unwrap();
    set_limit(&conn, "cost.monthly_limit_micros", "5000");
    set_limit(&conn, "cost.over_limit_policy", cost::POLICY_REDUCE_SEATS);

    let estimate = cost::estimate(&conn, 8, 4, 9).unwrap();
    let decision = cost::guard(&conn, &estimate).unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.max_seats, Some(cost::REDUCED_SEATS));
    assert_eq!(decision.max_rounds, None);
}
