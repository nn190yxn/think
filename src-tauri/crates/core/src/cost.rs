//! 成本治理：把模型与连接器调用换算成整数微元，按日汇总，超限时给出降级决定。
//!
//! 全部金额以整数微元记账，避免浮点累加误差。`cost_days` 在每次记账时按
//! UTC 日期 upsert，因此配额检查是常数时间，不需要扫描全量调用记录。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::council::tuning;
use crate::error::CoreResult;
use crate::llm::platform;

/// 未配置币种时的默认展示币种。
pub const DEFAULT_CURRENCY: &str = "CNY";

/// 估算时每次模型调用假定的提示词 token 数。
pub const ASSUMED_PROMPT_TOKENS_PER_CALL: i64 = 900;
/// 估算时每次模型调用假定的补全 token 数。
pub const ASSUMED_COMPLETION_TOKENS_PER_CALL: i64 = 300;

pub const POLICY_REJECT: &str = "reject";
pub const POLICY_REDUCE_ROUNDS: &str = "reduce_rounds";
pub const POLICY_REDUCE_SEATS: &str = "reduce_seats";

/// 降级策略压缩到的轮次与席位数。
pub const REDUCED_ROUNDS: i64 = 2;
pub const REDUCED_SEATS: i64 = 4;

/// 连接器每次调用单价的配置键。
const CONNECTOR_PRICE_KEY: &str = "costMicrosPerCall";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    pub llm_calls: i64,
    pub search_calls: i64,
    pub tokens: i64,
    pub cost_micros: i64,
    /// 参与估算的平台代码，未配置时为空串。
    pub platform_code: String,
    /// 是否至少配置了一项非零单价；为假时费用按零计。
    pub priced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaDecision {
    pub allowed: bool,
    pub policy: String,
    pub max_rounds: Option<i64>,
    pub max_seats: Option<i64>,
    pub reason: String,
    pub today_micros: i64,
    pub month_micros: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostDayView {
    pub day: String,
    pub calls: i64,
    pub tokens: i64,
    pub cost_micros: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostSummary {
    pub days: Vec<CostDayView>,
    pub today_micros: i64,
    pub month_micros: i64,
    pub daily_limit_micros: i64,
    pub monthly_limit_micros: i64,
    pub policy: String,
    pub currency: String,
    pub priced: bool,
}

fn now(conn: &Connection) -> CoreResult<String> {
    let value: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;
    Ok(value)
}

/// 每千 token 单价换算成一次调用的费用，整数除法向下取整。
fn tokens_cost_micros(prompt_tokens: i64, completion_tokens: i64, input_price: i64, output_price: i64) -> i64 {
    let prompt = prompt_tokens.max(0).saturating_mul(input_price.max(0));
    let completion = completion_tokens.max(0).saturating_mul(output_price.max(0));
    (prompt + completion) / 1000
}

/// 某个平台的一次调用费用。平台不存在或未配置单价时按零计。
pub fn llm_cost_micros(
    conn: &Connection,
    platform_code: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
) -> CoreResult<i64> {
    let prices: Option<(i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT input_price_micros_per_1k, output_price_micros_per_1k
             FROM ai_platforms WHERE code = ?1",
        )?;
        let mut rows = stmt.query_map([platform_code], |row| Ok((row.get(0)?, row.get(1)?)))?;
        match rows.next() {
            Some(row) => Some(row?),
            None => None,
        }
    };
    let (input_price, output_price) = prices.unwrap_or((0, 0));
    Ok(tokens_cost_micros(
        prompt_tokens,
        completion_tokens,
        input_price,
        output_price,
    ))
}

/// 某个连接器的一次调用费用。单价写在连接器 config 的 `costMicrosPerCall`。
pub fn connector_cost_micros(conn: &Connection, connector_id: Option<&str>) -> CoreResult<i64> {
    let Some(connector_id) = connector_id else {
        return Ok(0);
    };
    let config_json: Option<String> = {
        let mut stmt = conn.prepare("SELECT config_json FROM connectors WHERE id = ?1")?;
        let mut rows = stmt.query_map([connector_id], |row| row.get(0))?;
        match rows.next() {
            Some(row) => Some(row?),
            None => None,
        }
    };
    let Some(config_json) = config_json else {
        return Ok(0);
    };
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or(serde_json::Value::Null);
    Ok(config
        .get(CONNECTOR_PRICE_KEY)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
        .max(0))
}

/// 平台与连接器是否配置了至少一项非零单价。
fn is_priced(conn: &Connection) -> CoreResult<bool> {
    let platform_priced: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ai_platforms
         WHERE input_price_micros_per_1k > 0 OR output_price_micros_per_1k > 0",
        [],
        |row| row.get(0),
    )?;
    if platform_priced > 0 {
        return Ok(true);
    }
    let mut stmt = conn.prepare("SELECT config_json FROM connectors")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let config: serde_json::Value =
            serde_json::from_str(&row?).unwrap_or(serde_json::Value::Null);
        if config
            .get(CONNECTOR_PRICE_KEY)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
            > 0
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 把一次记账写进当日汇总，`calls`、`tokens` 与费用按增量累加。
fn upsert_day(conn: &Connection, calls: i64, tokens: i64, cost_micros: i64) -> CoreResult<()> {
    let updated_at = now(conn)?;
    conn.execute(
        "INSERT INTO cost_days (day, calls, tokens, cost_micros, updated_at)
         VALUES (date('now'), ?1, ?2, ?3, ?4)
         ON CONFLICT(day) DO UPDATE SET
             calls = calls + excluded.calls,
             tokens = tokens + excluded.tokens,
             cost_micros = cost_micros + excluded.cost_micros,
             updated_at = excluded.updated_at",
        rusqlite::params![calls, tokens, cost_micros, updated_at],
    )?;
    Ok(())
}

/// 记录一次模型调用费用，返回本次费用。
pub fn record_llm_cost(
    conn: &Connection,
    prompt_tokens: i64,
    completion_tokens: i64,
    platform_code: &str,
) -> CoreResult<i64> {
    let cost = llm_cost_micros(conn, platform_code, prompt_tokens, completion_tokens)?;
    upsert_day(conn, 1, prompt_tokens.max(0) + completion_tokens.max(0), cost)?;
    Ok(cost)
}

/// 记录一次连接器调用费用，返回本次费用。
pub fn record_connector_cost(conn: &Connection, connector_id: Option<&str>) -> CoreResult<i64> {
    let cost = connector_cost_micros(conn, connector_id)?;
    upsert_day(conn, 1, 0, cost)?;
    Ok(cost)
}

/// 当日与当月已发生的费用。
pub fn today_and_month_micros(conn: &Connection) -> CoreResult<(i64, i64)> {
    let today: i64 = conn.query_row(
        "SELECT COALESCE(SUM(cost_micros), 0) FROM cost_days WHERE day = date('now')",
        [],
        |row| row.get(0),
    )?;
    let month: i64 = conn.query_row(
        "SELECT COALESCE(SUM(cost_micros), 0) FROM cost_days
         WHERE substr(day, 1, 7) = strftime('%Y-%m', 'now')",
        [],
        |row| row.get(0),
    )?;
    Ok((today, month))
}

/// 发起前的费用估算：按席位与轮次估算模型调用，按检索次数估算连接器调用。
pub fn estimate(conn: &Connection, seats: i64, rounds: i64, searches: i64) -> CoreResult<CostEstimate> {
    let seats = seats.max(1);
    let rounds = rounds.max(1);
    // 每位席位每轮一次调用，另加一次收敛裁决。
    let llm_calls = seats * rounds + 1;
    estimate_calls(conn, llm_calls, searches)
}

/// 已知实际调用次数时的费用估算：回看单场会诊时用它，不再按席位与轮次推算。
pub fn estimate_calls(conn: &Connection, llm_calls: i64, searches: i64) -> CoreResult<CostEstimate> {
    let llm_calls = llm_calls.max(0);
    let searches = searches.max(0);
    let tokens = llm_calls * (ASSUMED_PROMPT_TOKENS_PER_CALL + ASSUMED_COMPLETION_TOKENS_PER_CALL);

    let platform = platform::enabled(conn)?;
    let platform_code = platform.as_ref().map(|item| item.code.clone()).unwrap_or_default();
    let (input_price, output_price) = platform
        .as_ref()
        .map(|item| (item.input_price_micros_per_1k, item.output_price_micros_per_1k))
        .unwrap_or((0, 0));
    let llm_micros = tokens_cost_micros(
        llm_calls * ASSUMED_PROMPT_TOKENS_PER_CALL,
        llm_calls * ASSUMED_COMPLETION_TOKENS_PER_CALL,
        input_price,
        output_price,
    );

    let search_connector = crate::connector::repo::enabled_of_kind(conn, crate::connector::KIND_SEARCH)?;
    let search_unit = connector_cost_micros(conn, search_connector.as_ref().map(|item| item.id.as_str()))?;
    let search_micros = search_unit.saturating_mul(searches);

    Ok(CostEstimate {
        llm_calls,
        search_calls: searches,
        tokens,
        cost_micros: llm_micros + search_micros,
        platform_code,
        priced: is_priced(conn)?,
    })
}

/// 配额闸门：未超限放行，超限按策略拒绝或压缩轮次与席位。
pub fn guard(conn: &Connection, estimate: &CostEstimate) -> CoreResult<QuotaDecision> {
    let daily_limit = tuning::int_of(conn, "cost.daily_limit_micros")?;
    let monthly_limit = tuning::int_of(conn, "cost.monthly_limit_micros")?;
    let policy = tuning::value_of(conn, "cost.over_limit_policy")?;
    let (today, month) = today_and_month_micros(conn)?;

    let over_daily = daily_limit > 0 && today.saturating_add(estimate.cost_micros) > daily_limit;
    let over_monthly =
        monthly_limit > 0 && month.saturating_add(estimate.cost_micros) > monthly_limit;
    if !over_daily && !over_monthly {
        return Ok(QuotaDecision {
            allowed: true,
            policy,
            max_rounds: None,
            max_seats: None,
            reason: "费用在限额之内".to_string(),
            today_micros: today,
            month_micros: month,
        });
    }

    let scope = if over_daily { "当日" } else { "当月" };
    let decision = match policy.as_str() {
        POLICY_REDUCE_ROUNDS => QuotaDecision {
            allowed: true,
            policy,
            max_rounds: Some(REDUCED_ROUNDS),
            max_seats: None,
            reason: format!(
                "已接近{scope}费用上限，本次会诊最多讨论 {REDUCED_ROUNDS} 轮"
            ),
            today_micros: today,
            month_micros: month,
        },
        POLICY_REDUCE_SEATS => QuotaDecision {
            allowed: true,
            policy,
            max_rounds: None,
            max_seats: Some(REDUCED_SEATS),
            reason: format!("已接近{scope}费用上限，本次会诊最多 {REDUCED_SEATS} 位席位"),
            today_micros: today,
            month_micros: month,
        },
        _ => QuotaDecision {
            allowed: false,
            policy: POLICY_REJECT.to_string(),
            max_rounds: None,
            max_seats: None,
            reason: format!("{scope}费用已达上限，按当前策略拒绝发起会诊"),
            today_micros: today,
            month_micros: month,
        },
    };
    Ok(decision)
}

/// 最近若干天的费用汇总，附带当日与当月累计、上限与策略。
pub fn cost_summary(conn: &Connection, days: i64) -> CoreResult<CostSummary> {
    let days = days.clamp(1, 365);
    let mut stmt = conn.prepare(
        "SELECT day, calls, tokens, cost_micros FROM cost_days
         ORDER BY day DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([days], |row| {
        Ok(CostDayView {
            day: row.get(0)?,
            calls: row.get(1)?,
            tokens: row.get(2)?,
            cost_micros: row.get(3)?,
        })
    })?;
    let mut list = Vec::new();
    for row in rows {
        list.push(row?);
    }
    let (today, month) = today_and_month_micros(conn)?;
    let currency = platform::enabled(conn)?
        .map(|item| item.currency)
        .unwrap_or_else(|| DEFAULT_CURRENCY.to_string());
    Ok(CostSummary {
        days: list,
        today_micros: today,
        month_micros: month,
        daily_limit_micros: tuning::int_of(conn, "cost.daily_limit_micros")?,
        monthly_limit_micros: tuning::int_of(conn, "cost.monthly_limit_micros")?,
        policy: tuning::value_of(conn, "cost.over_limit_policy")?,
        currency,
        priced: is_priced(conn)?,
    })
}
