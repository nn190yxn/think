//! 分歧判定：词面重合度与极性判定的混合计算。
//!
//! 词面重合度会把「应该提高定价」与「不应该提高定价」判为高度一致，因为两者
//! 共享几乎全部词。修正办法是对高重合的席位对追加一次极性判定，判定为对立时
//! 把该对的相似度按零计入均值。极性判定不可用时只降级不中断，并留下 `fell_back`。

use rusqlite::Connection;

use crate::error::{CoreError, CoreResult};
use crate::llm::{call_model, ModelClient, ModelRequest, RetryPolicy};

use super::{scoring, tuning};

/// 分歧判定方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceMode {
    /// 只用词面重合度。
    Lexical,
    /// 对高重合席位对做极性判定，判定为对立时按零计。
    Polarity,
    /// 先词面、再对高重合对做极性判定。
    Hybrid,
}

impl DivergenceMode {
    pub fn parse(value: &str) -> Option<DivergenceMode> {
        match value.trim().to_ascii_lowercase().as_str() {
            "lexical" => Some(DivergenceMode::Lexical),
            "polarity" => Some(DivergenceMode::Polarity),
            "hybrid" => Some(DivergenceMode::Hybrid),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DivergenceMode::Lexical => "lexical",
            DivergenceMode::Polarity => "polarity",
            DivergenceMode::Hybrid => "hybrid",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DivergenceMode::Lexical => "词面判定",
            DivergenceMode::Polarity => "极性判定",
            DivergenceMode::Hybrid => "混合判定",
        }
    }
}

/// 极性判定抽象：判断两条发言在结论方向上是否相反。
///
/// 真实实现由外壳注入模型调用；测试用脚本化实现，不依赖网络。
pub trait PolarityJudge {
    fn opposing(&self, left: &str, right: &str) -> CoreResult<bool>;
}

/// 极性判定的调用用途标识，写入 `llm_calls` 便于与会诊发言区分。
pub const POLARITY_PURPOSE: &str = "council_polarity";

const POLARITY_SYSTEM: &str = "你是一名严格的立场校对员。只判断两段发言在结论方向上是否相反，不做评价、不给建议。";

/// 用模型实现的极性判定。联网关闭或平台未配置时调用失败，由上层回退词面判定。
pub struct ModelPolarityJudge<'a> {
    pub conn: &'a Connection,
    pub client: &'a dyn ModelClient,
    pub policy: RetryPolicy,
}

impl ModelPolarityJudge<'_> {
    /// 从模型答复里解析判定结论。无法解析时返回错误，交由上层回退并留痕。
    pub fn parse_verdict(content: &str) -> Option<bool> {
        let first_line = content.lines().find(|line| !line.trim().is_empty())?.trim();
        for marker in ["不对立", "不相反", "一致", "不冲突"] {
            if first_line.starts_with(marker) {
                return Some(false);
            }
        }
        for marker in ["对立", "相反", "冲突"] {
            if first_line.starts_with(marker) {
                return Some(true);
            }
        }
        None
    }
}

impl PolarityJudge for ModelPolarityJudge<'_> {
    fn opposing(&self, left: &str, right: &str) -> CoreResult<bool> {
        let user = format!(
            "甲：{left}\n\n乙：{right}\n\n若两者结论方向相反，首行只答「对立」；否则首行只答「一致」。"
        );
        let request = ModelRequest::new(POLARITY_PURPOSE, POLARITY_SYSTEM, user)
            .with_prompt_version(super::orchestrator::PROMPT_VERSION);
        let response = call_model(self.conn, self.client, &request, &self.policy)?;
        Self::parse_verdict(&response.content).ok_or_else(|| {
            CoreError::MalformedResponse("极性判定未能给出「对立」或「一致」".to_string())
        })
    }
}

/// 一轮发言的分歧统计。
#[derive(Debug, Clone)]
pub struct MetricStats {
    pub participant_count: i64,
    pub avg_similarity: f64,
    pub min_similarity: f64,
    pub divergence: f64,
    pub method: String,
    pub fell_back: bool,
}

/// 计算一轮发言的分歧指标。席位数少于 2 时分歧度记 0，不触发追加。
pub fn round_metric(
    conn: &Connection,
    judge: Option<&dyn PolarityJudge>,
    mode: DivergenceMode,
    answers: &[(String, String)],
) -> CoreResult<MetricStats> {
    let count = answers.len();
    if count < 2 {
        return Ok(MetricStats {
            participant_count: count as i64,
            avg_similarity: 0.0,
            min_similarity: 0.0,
            divergence: 0.0,
            method: mode.as_str().to_string(),
            fell_back: false,
        });
    }

    let token_sets: Vec<std::collections::BTreeSet<String>> = answers
        .iter()
        .map(|(_, content)| scoring::tokens(content))
        .collect();

    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    let mut total = 0.0;
    let mut minimum = f64::MAX;
    for left in 0..count {
        for right in (left + 1)..count {
            let similarity = scoring::overlap(&token_sets[left], &token_sets[right]);
            pairs.push((left, right, similarity));
            total += similarity;
            minimum = minimum.min(similarity);
        }
    }
    let lexical_avg = total / pairs.len() as f64;
    let lexical_min = minimum;

    if mode == DivergenceMode::Lexical {
        return Ok(stats(
            count as i64,
            lexical_avg,
            lexical_min,
            "lexical",
            false,
        ));
    }

    // 只对高重合的席位对做极性判定，对数设上限以约束模型调用。
    let threshold = tuning::float_of(conn, "council.polarity_min_similarity")?;
    let max_pairs = tuning::int_of(conn, "council.polarity_max_pairs")?.clamp(0, 20) as usize;
    let mut selected: Vec<(usize, usize, usize, f64)> = pairs
        .iter()
        .enumerate()
        .filter(|(_, (_, _, similarity))| *similarity >= threshold)
        .map(|(index, (left, right, similarity))| (index, *left, *right, *similarity))
        .collect();
    selected.sort_by(|left, right| {
        right
            .3
            .partial_cmp(&left.3)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected.truncate(max_pairs);

    if selected.is_empty() {
        return Ok(stats(
            count as i64,
            lexical_avg,
            lexical_min,
            mode.as_str(),
            false,
        ));
    }

    let Some(judge) = judge else {
        // 极性判定不可用：回退词面判定并留痕。
        return Ok(stats(
            count as i64,
            lexical_avg,
            lexical_min,
            "lexical",
            true,
        ));
    };

    let mut adjusted: Vec<f64> = pairs.iter().map(|(_, _, value)| *value).collect();
    for (index, left, right, _) in &selected {
        match judge.opposing(&answers[*left].1, &answers[*right].1) {
            Ok(true) => {
                // 判定为对立的对，其相似度按零参与均值。
                adjusted[*index] = 0.0;
            }
            Ok(false) => {}
            Err(_) => {
                return Ok(stats(
                    count as i64,
                    lexical_avg,
                    lexical_min,
                    "lexical",
                    true,
                ))
            }
        }
    }

    let avg = adjusted.iter().sum::<f64>() / adjusted.len() as f64;
    let min = adjusted.iter().copied().fold(f64::MAX, f64::min);
    Ok(stats(count as i64, avg, min, mode.as_str(), false))
}

fn stats(
    participant_count: i64,
    avg_similarity: f64,
    min_similarity: f64,
    method: &str,
    fell_back: bool,
) -> MetricStats {
    MetricStats {
        participant_count,
        avg_similarity,
        min_similarity,
        divergence: tuning::divergence_of(avg_similarity),
        method: method.to_string(),
        fell_back,
    }
}
