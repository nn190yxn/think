//! 调参服务：把写在代码里的调节常数交给用户，并在写入前整体校验。
//!
//! 取值落在 `settings` 表，键名与调参项标识一致。设置缺失或不可解析时
//! 按声明默认值生效，不阻断调用；一次提交中的任一项非法时整批不落库。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

use super::scoring;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TuningKind {
    Int,
    Float,
    Bool,
    Text,
}

impl TuningKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TuningKind::Int => "int",
            TuningKind::Float => "float",
            TuningKind::Bool => "bool",
            TuningKind::Text => "text",
        }
    }
}

/// 一项调参的声明。`default_value` 与取值都以字符串保存，读取时按 `kind` 解析。
pub struct TuningSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub unit: &'static str,
    pub kind: TuningKind,
    pub default_value: &'static str,
    pub min: f64,
    pub max: f64,
    pub note: &'static str,
}

/// 允许的取值集合，用于 `Text` 型调参。
fn allowed_values(key: &str) -> Option<&'static [&'static str]> {
    match key {
        "connector.query_mode" => Some(&["keyword", "question"]),
        "council.divergence_mode" => Some(&["lexical", "polarity", "hybrid"]),
        "cost.over_limit_policy" => Some(&["reject", "reduce_rounds", "reduce_seats"]),
        _ => None,
    }
}

/// P10 交付的十一项。后续阶段按设计文档在本表追加，已发布的项不再改动语义。
pub const SPECS: &[TuningSpec] = &[
    TuningSpec {
        key: "council.max_rounds",
        label: "讨论轮次上限",
        group: "会诊",
        unit: "轮",
        kind: TuningKind::Int,
        default_value: "3",
        min: 2.0,
        max: 4.0,
        note: "含作答轮，分歧大时最多追加到此轮数",
    },
    TuningSpec {
        key: "council.divergence_threshold",
        label: "追加轮次阈值",
        group: "会诊",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.6",
        min: 0.1,
        max: 0.9,
        note: "本轮分歧度高于此值才追加质询轮",
    },
    TuningSpec {
        key: "council.convergence_delta",
        label: "收敛判定差值",
        group: "会诊",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.05",
        min: 0.01,
        max: 0.3,
        note: "分歧度下降达到此值判定为收敛",
    },
    TuningSpec {
        key: "activation_half_life_hours",
        label: "激活半衰期",
        group: "网络",
        unit: "小时",
        kind: TuningKind::Float,
        default_value: "168",
        min: 1.0,
        max: 8760.0,
        note: "沿用既有键名，激活随时间的衰减速度",
    },
    TuningSpec {
        key: "network.neighbor_factor",
        label: "一跳折半系数",
        group: "网络",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.5",
        min: 0.0,
        max: 1.0,
        note: "第一跳邻居分得的增益比例",
    },
    TuningSpec {
        key: "network.hop_count",
        label: "传播跳数",
        group: "网络",
        unit: "跳",
        kind: TuningKind::Int,
        default_value: "2",
        min: 1.0,
        max: 3.0,
        note: "一次唤起最多传到第几跳",
    },
    TuningSpec {
        key: "network.hop_decay",
        label: "跳间衰减",
        group: "网络",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.5",
        min: 0.0,
        max: 1.0,
        note: "第二跳及其后每跳的增益再乘此系数",
    },
    TuningSpec {
        key: "network.min_edge_weight",
        label: "最小连线权重",
        group: "网络",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.05",
        min: 0.0,
        max: 1.0,
        note: "低于此值的连线不参与衰减与聚类",
    },
    TuningSpec {
        key: "network.merge_similarity",
        label: "节点合并相似度",
        group: "网络",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.85",
        min: 0.5,
        max: 1.0,
        note: "内容重合高于此值的节点合并为同一节点",
    },
    TuningSpec {
        key: "network.cluster_enabled",
        label: "固化时聚类",
        group: "网络",
        unit: "",
        kind: TuningKind::Bool,
        default_value: "true",
        min: 0.0,
        max: 1.0,
        note: "固化完成后是否重新划分认知社区",
    },
    TuningSpec {
        key: "network.cluster_min_size",
        label: "最小团规模",
        group: "网络",
        unit: "节点",
        kind: TuningKind::Int,
        default_value: "3",
        min: 2.0,
        max: 50.0,
        note: "成员数少于此值的社区不呈现给用户",
    },
    TuningSpec {
        key: "connector.max_results",
        label: "单次检索结果数",
        group: "连接器",
        unit: "条",
        kind: TuningKind::Int,
        default_value: "6",
        min: 1.0,
        max: 20.0,
        note: "单次检索结果数上限",
    },
    TuningSpec {
        key: "connector.max_searches_per_session",
        label: "每次会诊检索次数",
        group: "连接器",
        unit: "次",
        kind: TuningKind::Int,
        default_value: "12",
        min: 0.0,
        max: 40.0,
        note: "每次会诊检索次数上限",
    },
    TuningSpec {
        key: "connector.snapshot_body",
        label: "保存网页正文",
        group: "连接器",
        unit: "",
        kind: TuningKind::Bool,
        default_value: "false",
        min: 0.0,
        max: 1.0,
        note: "是否保存网页正文快照，关闭时只留摘要",
    },
    TuningSpec {
        key: "connector.timeout_secs",
        label: "连接器超时",
        group: "连接器",
        unit: "秒",
        kind: TuningKind::Int,
        default_value: "15",
        min: 3.0,
        max: 60.0,
        note: "单次连接器调用超时",
    },
    TuningSpec {
        key: "council.shared_background",
        label: "先做共享背景检索",
        group: "会诊",
        unit: "",
        kind: TuningKind::Bool,
        default_value: "true",
        min: 0.0,
        max: 1.0,
        note: "会诊开始前是否为全部席位取一份共同背景",
    },
    TuningSpec {
        key: "council.seat_search",
        label: "允许席位补充检索",
        group: "会诊",
        unit: "",
        kind: TuningKind::Bool,
        default_value: "false",
        min: 0.0,
        max: 1.0,
        note: "是否允许单个席位按自己的角度补充检索",
    },
    TuningSpec {
        key: "connector.query_mode",
        label: "对外发送模式",
        group: "连接器",
        unit: "",
        kind: TuningKind::Text,
        default_value: "keyword",
        min: 0.0,
        max: 1.0,
        note: "发送关键词还是脱敏后的问句原文",
    },
    TuningSpec {
        key: "connector.preflight",
        label: "发送前预演确认",
        group: "连接器",
        unit: "",
        kind: TuningKind::Bool,
        default_value: "true",
        min: 0.0,
        max: 1.0,
        note: "开启时手动检索与连通测试先返回待确认内容，确认后才真正发送",
    },
    TuningSpec {
        key: "council.divergence_mode",
        label: "分歧判定方式",
        group: "会诊",
        unit: "",
        kind: TuningKind::Text,
        default_value: "hybrid",
        min: 0.0,
        max: 1.0,
        note: "词面判定、极性判定或两者混合",
    },
    TuningSpec {
        key: "council.polarity_min_similarity",
        label: "极性判定重合度下限",
        group: "会诊",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.7",
        min: 0.3,
        max: 1.0,
        note: "重合度不低于此值的席位对才追加极性判定",
    },
    TuningSpec {
        key: "council.polarity_max_pairs",
        label: "极性判定对数上限",
        group: "会诊",
        unit: "对",
        kind: TuningKind::Int,
        default_value: "6",
        min: 0.0,
        max: 20.0,
        note: "单轮极性判定的席位对上限，控制模型调用量",
    },
    TuningSpec {
        key: "cost.daily_limit_micros",
        label: "日费用上限",
        group: "成本",
        unit: "微元",
        kind: TuningKind::Int,
        default_value: "0",
        min: 0.0,
        max: 1_000_000_000.0,
        note: "当日费用上限，0 表示不限",
    },
    TuningSpec {
        key: "cost.monthly_limit_micros",
        label: "月费用上限",
        group: "成本",
        unit: "微元",
        kind: TuningKind::Int,
        default_value: "0",
        min: 0.0,
        max: 10_000_000_000.0,
        note: "当月费用上限，0 表示不限",
    },
    TuningSpec {
        key: "cost.over_limit_policy",
        label: "超限处理方式",
        group: "成本",
        unit: "",
        kind: TuningKind::Text,
        default_value: "reject",
        min: 0.0,
        max: 1.0,
        note: "超限时拒绝发起、压缩轮次还是压缩席位",
    },
    TuningSpec {
        key: "backup.keep_count",
        label: "保留备份份数",
        group: "备份",
        unit: "份",
        kind: TuningKind::Int,
        default_value: "10",
        min: 1.0,
        max: 100.0,
        note: "超出份数的旧备份只标记移除，不再占用新文件",
    },
    TuningSpec {
        key: "backup.before_migration",
        label: "迁移前自动备份",
        group: "备份",
        unit: "",
        kind: TuningKind::Bool,
        default_value: "true",
        min: 0.0,
        max: 1.0,
        note: "版本提升前先创建一份 pre_migration 备份，失败则中止迁移",
    },
    TuningSpec {
        key: "snapshot.retain_days",
        label: "正文快照保留天数",
        group: "备份",
        unit: "天",
        kind: TuningKind::Int,
        default_value: "180",
        min: 7.0,
        max: 3650.0,
        note: "清理超期网页正文快照，来源元数据行保留",
    },
    TuningSpec {
        key: "echo.threshold",
        label: "回音提示阈值",
        group: "会诊",
        unit: "",
        kind: TuningKind::Float,
        default_value: "0.8",
        min: 0.5,
        max: 1.0,
        note: "结论与既有原则重合度达到此值才提示可能是自我确认",
    },
    TuningSpec {
        key: "council.stale_heartbeat_seconds",
        label: "中断判定心跳间隔",
        group: "会诊",
        unit: "秒",
        kind: TuningKind::Int,
        default_value: "120",
        min: 30.0,
        max: 3600.0,
        note: "运行中会话的心跳早于该间隔即视为已中断",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuningItem {
    pub key: String,
    pub label: String,
    pub group: String,
    pub unit: String,
    pub kind: String,
    pub value: String,
    pub default_value: String,
    pub min: f64,
    pub max: f64,
    pub note: String,
    /// 当前值是否偏离默认值。
    pub customized: bool,
}

fn spec_of(key: &str) -> Option<&'static TuningSpec> {
    SPECS.iter().find(|spec| spec.key == key)
}

/// 校验并归一化单个取值，返回可落库的标准写法。
pub fn normalize_value(key: &str, value: &str) -> CoreResult<String> {
    let spec = spec_of(key)
        .ok_or_else(|| CoreError::InvalidInput(format!("未知的调参项：{key}")))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "调参项「{}」的取值不能为空",
            spec.label
        )));
    }

    let out_of_range = |allowed: &str| {
        CoreError::InvalidInput(format!(
            "调参项「{}」的取值 {trimmed} 超出允许范围 {allowed}",
            spec.label
        ))
    };

    match spec.kind {
        TuningKind::Bool => match trimmed.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok("true".to_string()),
            "false" | "0" | "no" | "off" => Ok("false".to_string()),
            _ => Err(CoreError::InvalidInput(format!(
                "调参项「{}」只接受 true 或 false",
                spec.label
            ))),
        },
        TuningKind::Int => {
            let parsed = trimmed
                .parse::<i64>()
                .map_err(|_| out_of_range(&format!("{} 至 {}", spec.min as i64, spec.max as i64)))?;
            if (parsed as f64) < spec.min || (parsed as f64) > spec.max {
                return Err(out_of_range(&format!(
                    "{} 至 {}",
                    spec.min as i64, spec.max as i64
                )));
            }
            Ok(parsed.to_string())
        }
        TuningKind::Float => {
            let parsed = trimmed.parse::<f64>().map_err(|_| {
                out_of_range(&format!("{} 至 {}", spec.min, spec.max))
            })?;
            if !parsed.is_finite() || parsed < spec.min || parsed > spec.max {
                return Err(out_of_range(&format!("{} 至 {}", spec.min, spec.max)));
            }
            Ok(trimmed.to_string())
        }
        TuningKind::Text => match allowed_values(key) {
            Some(options) => {
                let lowered = trimmed.to_ascii_lowercase();
                if options.contains(&lowered.as_str()) {
                    Ok(lowered)
                } else {
                    Err(CoreError::InvalidInput(format!(
                        "调参项「{}」只接受 {}",
                        spec.label,
                        options.join("、")
                    )))
                }
            }
            None => Ok(trimmed.to_string()),
        },
    }
}

/// 读取单个调参的当前值，缺失或不可解析时退回默认值。
pub fn value_of(conn: &Connection, key: &str) -> CoreResult<String> {
    let spec = spec_of(key)
        .ok_or_else(|| CoreError::InvalidInput(format!("未知的调参项：{key}")))?;
    let stored = crate::db::settings::get(conn, key)?;
    match stored.and_then(|raw| normalize_value(key, &raw).ok()) {
        Some(value) => Ok(value),
        None => Ok(spec.default_value.to_string()),
    }
}

pub fn bool_of(conn: &Connection, key: &str) -> CoreResult<bool> {
    Ok(value_of(conn, key)? == "true")
}

pub fn int_of(conn: &Connection, key: &str) -> CoreResult<i64> {
    let raw = value_of(conn, key)?;
    raw.parse::<i64>().map_err(|_| {
        CoreError::InvalidInput(format!("调参项「{key}」不是整数：{raw}"))
    })
}

pub fn float_of(conn: &Connection, key: &str) -> CoreResult<f64> {
    let raw = value_of(conn, key)?;
    raw.parse::<f64>()
        .map_err(|_| CoreError::InvalidInput(format!("调参项「{key}」不是数值：{raw}")))
}

fn item_of(conn: &Connection, spec: &TuningSpec) -> CoreResult<TuningItem> {
    let value = value_of(conn, spec.key)?;
    Ok(TuningItem {
        key: spec.key.to_string(),
        label: spec.label.to_string(),
        group: spec.group.to_string(),
        unit: spec.unit.to_string(),
        kind: spec.kind.as_str().to_string(),
        customized: value != spec.default_value,
        value,
        default_value: spec.default_value.to_string(),
        min: spec.min,
        max: spec.max,
        note: spec.note.to_string(),
    })
}

/// 全部调参项及其当前值，按分组与声明顺序排列。
pub fn list(conn: &Connection) -> CoreResult<Vec<TuningItem>> {
    SPECS.iter().map(|spec| item_of(conn, spec)).collect()
}

/// 批量写入调参。任一项未知或越界时整批不落库。
pub fn set(conn: &Connection, updates: &[(String, String)]) -> CoreResult<Vec<TuningItem>> {
    if updates.is_empty() {
        return Err(CoreError::InvalidInput("调参提交不能为空".to_string()));
    }

    let mut normalized: Vec<(&'static str, String)> = Vec::with_capacity(updates.len());
    let mut seen: Vec<&str> = Vec::with_capacity(updates.len());
    for (key, value) in updates {
        let spec = spec_of(key)
            .ok_or_else(|| CoreError::InvalidInput(format!("未知的调参项：{key}")))?;
        if seen.contains(&spec.key) {
            return Err(CoreError::InvalidInput(format!(
                "调参项「{}」在一次提交中重复出现",
                spec.label
            )));
        }
        seen.push(spec.key);
        normalized.push((spec.key, normalize_value(spec.key, value)?));
    }

    let tx = conn.unchecked_transaction()?;
    for (key, value) in &normalized {
        crate::db::settings::set(&tx, key, value)?;
    }
    tx.commit()?;

    list(conn)
}

/// 会诊与网络的调节常数快照，避免在一次流程中反复读设置。
#[derive(Debug, Clone)]
pub struct TuningSnapshot {
    pub max_rounds: i64,
    pub divergence_threshold: f64,
    pub convergence_delta: f64,
    pub neighbor_factor: f64,
    pub hop_count: i64,
    pub hop_decay: f64,
    pub min_edge_weight: f64,
    pub merge_similarity: f64,
    pub cluster_enabled: bool,
    pub cluster_min_size: usize,
    pub connector_max_results: i64,
    pub connector_max_searches_per_session: i64,
    pub connector_snapshot_body: bool,
    pub connector_timeout_secs: i64,
    pub shared_background: bool,
    pub seat_search: bool,
    pub query_mode: String,
    pub preflight: bool,
    pub divergence_mode: String,
    pub polarity_min_similarity: f64,
    pub polarity_max_pairs: i64,
    pub daily_limit_micros: i64,
    pub monthly_limit_micros: i64,
    pub over_limit_policy: String,
    pub backup_keep_count: i64,
    pub backup_before_migration: bool,
    pub snapshot_retain_days: i64,
    pub echo_threshold: f64,
    pub stale_heartbeat_seconds: i64,
}

pub fn snapshot(conn: &Connection) -> CoreResult<TuningSnapshot> {
    Ok(TuningSnapshot {
        max_rounds: int_of(conn, "council.max_rounds")?,
        divergence_threshold: float_of(conn, "council.divergence_threshold")?,
        convergence_delta: float_of(conn, "council.convergence_delta")?,
        neighbor_factor: float_of(conn, "network.neighbor_factor")?,
        hop_count: int_of(conn, "network.hop_count")?,
        hop_decay: float_of(conn, "network.hop_decay")?,
        min_edge_weight: float_of(conn, "network.min_edge_weight")?,
        merge_similarity: float_of(conn, "network.merge_similarity")?,
        cluster_enabled: bool_of(conn, "network.cluster_enabled")?,
        cluster_min_size: int_of(conn, "network.cluster_min_size")? as usize,
        connector_max_results: int_of(conn, "connector.max_results")?,
        connector_max_searches_per_session: int_of(conn, "connector.max_searches_per_session")?,
        connector_snapshot_body: bool_of(conn, "connector.snapshot_body")?,
        connector_timeout_secs: int_of(conn, "connector.timeout_secs")?,
        shared_background: bool_of(conn, "council.shared_background")?,
        seat_search: bool_of(conn, "council.seat_search")?,
        query_mode: value_of(conn, "connector.query_mode")?,
        preflight: bool_of(conn, "connector.preflight")?,
        divergence_mode: value_of(conn, "council.divergence_mode")?,
        polarity_min_similarity: float_of(conn, "council.polarity_min_similarity")?,
        polarity_max_pairs: int_of(conn, "council.polarity_max_pairs")?,
        daily_limit_micros: int_of(conn, "cost.daily_limit_micros")?,
        monthly_limit_micros: int_of(conn, "cost.monthly_limit_micros")?,
        over_limit_policy: value_of(conn, "cost.over_limit_policy")?,
        backup_keep_count: int_of(conn, "backup.keep_count")?,
        backup_before_migration: bool_of(conn, "backup.before_migration")?,
        snapshot_retain_days: int_of(conn, "snapshot.retain_days")?,
        echo_threshold: float_of(conn, "echo.threshold")?,
        stale_heartbeat_seconds: int_of(conn, "council.stale_heartbeat_seconds")?,
    })
}

/// 本轮分歧度：一减平均用词重合度，落在 0 至 1。
pub fn divergence_of(avg_similarity: f64) -> f64 {
    (1.0 - avg_similarity).clamp(0.0, 1.0)
}

/// 是否收敛：分歧度已不高于追加阈值，或相比上一轮下降达到收敛差值。
pub fn converged_of(
    divergence: f64,
    previous: Option<f64>,
    threshold: f64,
    delta: f64,
) -> bool {
    if divergence <= threshold {
        return true;
    }
    matches!(previous, Some(previous) if previous - divergence >= delta)
}

/// 是否追加下一轮：未达轮次上限、本轮分歧仍高于阈值、成功发言不少于两席。
pub fn should_append(
    round: i64,
    max_rounds: i64,
    divergence: f64,
    threshold: f64,
    participant_count: usize,
) -> bool {
    round < max_rounds && divergence > threshold && participant_count >= 2
}

/// 一轮发言的相似度统计，供轮次指标复用。
pub fn similarity_stats(answers: &[(String, String)]) -> (usize, f64, f64) {
    let count = answers.len();
    if count < 2 {
        return (count, 0.0, 0.0);
    }
    let token_sets: Vec<std::collections::BTreeSet<String>> = answers
        .iter()
        .map(|(_, content)| scoring::tokens(content))
        .collect();

    let mut total = 0.0;
    let mut minimum = f64::MAX;
    let mut pairs = 0usize;
    for left in 0..count {
        for right in (left + 1)..count {
            let similarity = scoring::overlap(&token_sets[left], &token_sets[right]);
            total += similarity;
            minimum = minimum.min(similarity);
            pairs += 1;
        }
    }
    if pairs == 0 {
        return (count, 0.0, 0.0);
    }
    (count, total / pairs as f64, minimum)
}
