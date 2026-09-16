//! 调参服务：默认值回退、范围校验、整体原子性与取值解析。

use proptest::prelude::*;
use thought_forge_core::council::tuning;
use thought_forge_core::db::{self, migrations};

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

#[test]
fn list_returns_the_declared_defaults() {
    let conn = db();
    let items = tuning::list(&conn).unwrap();
    assert_eq!(items.len(), tuning::SPECS.len());
    assert_eq!(
        items.len(),
        30,
        "P10 十一项 + P12 六项 + P13 五项 + P14 六项 + P15 回音阈值与中断心跳两项"
    );
    for item in &items {
        assert_eq!(item.value, item.default_value, "缺省时取默认值：{}", item.key);
        assert!(!item.customized);
    }

    // P12 新增：连接器上限、正文快照与共享背景开关。
    let keys: Vec<&str> = items.iter().map(|item| item.key.as_str()).collect();
    for key in [
        "connector.max_results",
        "connector.max_searches_per_session",
        "connector.snapshot_body",
        "connector.timeout_secs",
        "council.shared_background",
        "council.seat_search",
    ] {
        assert!(keys.contains(&key), "应包含 P12 调参项 {key}");
    }

    // P13 新增：检索发送模式与分歧判定方式。
    for key in [
        "connector.query_mode",
        "connector.preflight",
        "council.divergence_mode",
        "council.polarity_min_similarity",
        "council.polarity_max_pairs",
    ] {
        assert!(keys.contains(&key), "应包含 P13 调参项 {key}");
    }

    // P14 新增：成本上限与超限策略、备份保留与快照天数。
    for key in [
        "cost.daily_limit_micros",
        "cost.monthly_limit_micros",
        "cost.over_limit_policy",
        "backup.keep_count",
        "backup.before_migration",
        "snapshot.retain_days",
    ] {
        assert!(keys.contains(&key), "应包含 P14 调参项 {key}");
    }

    // P15 新增：回音阈值与中断心跳间隔。
    for key in ["echo.threshold", "council.stale_heartbeat_seconds"] {
        assert!(keys.contains(&key), "应包含 P15 调参项 {key}");
    }
}

#[test]
fn set_rejects_out_of_range_as_a_whole_batch() {
    let conn = db();
    let error = tuning::set(
        &conn,
        &[
            ("council.max_rounds".to_string(), "4".to_string()),
            ("network.hop_count".to_string(), "9".to_string()),
        ],
    )
    .unwrap_err();
    assert!(matches!(error, thought_forge_core::CoreError::InvalidInput(_)));
    // 同批合法的项也不得落库。
    assert_eq!(tuning::value_of(&conn, "council.max_rounds").unwrap(), "3");
}

#[test]
fn set_rejects_unknown_or_duplicate_keys() {
    let conn = db();
    assert!(tuning::set(&conn, &[("nope.key".to_string(), "1".to_string())]).is_err());
    assert!(tuning::set(
        &conn,
        &[
            ("council.max_rounds".to_string(), "2".to_string()),
            ("council.max_rounds".to_string(), "3".to_string()),
        ],
    )
    .is_err());
}

#[test]
fn set_normalizes_booleans_and_marks_customized() {
    let conn = db();
    tuning::set(&conn, &[("network.cluster_enabled".to_string(), "off".to_string())]).unwrap();
    assert!(!tuning::bool_of(&conn, "network.cluster_enabled").unwrap());
    let item = tuning::list(&conn)
        .unwrap()
        .into_iter()
        .find(|item| item.key == "network.cluster_enabled")
        .unwrap();
    assert_eq!(item.value, "false");
    assert!(item.customized);
}

#[test]
fn unparsable_stored_value_falls_back_to_default() {
    let conn = db();
    db::settings::set(&conn, "network.hop_count", "很多").unwrap();
    assert_eq!(tuning::int_of(&conn, "network.hop_count").unwrap(), 2);
    assert_eq!(tuning::value_of(&conn, "network.hop_count").unwrap(), "2");
}

#[test]
fn snapshot_reflects_written_values() {
    let conn = db();
    tuning::set(
        &conn,
        &[
            ("council.max_rounds".to_string(), "4".to_string()),
            ("network.hop_count".to_string(), "3".to_string()),
            ("network.hop_decay".to_string(), "0.25".to_string()),
        ],
    )
    .unwrap();
    let snapshot = tuning::snapshot(&conn).unwrap();
    assert_eq!(snapshot.max_rounds, 4);
    assert_eq!(snapshot.hop_count, 3);
    assert!((snapshot.hop_decay - 0.25).abs() < 1e-9);
    assert!(snapshot.cluster_enabled);
}

#[test]
fn append_and_convergence_rules_are_explicit() {
    // 分歧高于阈值且仍有余量时追加。
    assert!(tuning::should_append(2, 3, 0.8, 0.6, 6));
    // 达到轮次上限不追加。
    assert!(!tuning::should_append(3, 3, 0.8, 0.6, 6));
    // 成功发言不足两席不追加。
    assert!(!tuning::should_append(2, 4, 0.8, 0.6, 1));
    // 分歧不高于阈值即视为收敛。
    assert!(tuning::converged_of(0.5, None, 0.6, 0.05));
    // 下降达到收敛差值即视为收敛。
    assert!(tuning::converged_of(0.7, Some(0.8), 0.6, 0.05));
    assert!(!tuning::converged_of(0.7, Some(0.71), 0.6, 0.05));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// property 6：写入的调参值落在其声明的范围内。
    #[test]
    fn accepted_values_stay_within_range(rounds in 2i64..=4) {
        let conn = db();
        tuning::set(&conn, &[("council.max_rounds".to_string(), rounds.to_string())]).unwrap();
        let value = tuning::int_of(&conn, "council.max_rounds").unwrap();
        prop_assert!((2..=4).contains(&value));
    }

    /// property 5 的取值侧：越界值总被拒绝且不改动已存值。
    #[test]
    fn out_of_range_values_are_rejected(bad in 5i64..=100) {
        let conn = db();
        tuning::set(&conn, &[("council.max_rounds".to_string(), "4".to_string())]).unwrap();
        let result = tuning::set(&conn, &[("council.max_rounds".to_string(), bad.to_string())]);
        prop_assert!(result.is_err());
        prop_assert_eq!(tuning::value_of(&conn, "council.max_rounds").unwrap(), "4");
    }

    /// property 4：分歧越高越倾向于追加，越接近上限越倾向于收束。
    #[test]
    fn append_decision_is_monotone(
        low in 0.0f64..=1.0,
        spread in 0.0f64..=1.0,
        round in 1i64..=4,
    ) {
        let high = (low + spread).min(1.0);
        let threshold = 0.6;
        // 分歧上升不会把已经决定的追加改回不追加。
        if tuning::should_append(round, 4, low, threshold, 6) {
            prop_assert!(tuning::should_append(round, 4, high, threshold, 6));
        }
        // 更晚的轮次不会比更早的轮次更倾向于追加。
        if tuning::should_append(round + 1, 4, high, threshold, 6) {
            prop_assert!(tuning::should_append(round, 4, high, threshold, 6));
        }
    }
}
