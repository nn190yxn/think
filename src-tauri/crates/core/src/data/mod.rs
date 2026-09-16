//! 数据主权：把全部本地数据导出成可迁移归档，或在确认后清除。
//!
//! 归档覆盖除迁移账本以外的全部业务表；清除同样落在这些表上，清完再记一条
//! 审计，保证「清除」这个动作本身也可回看。

pub mod service;

use serde::{Deserialize, Serialize};

/// 导出归档格式标识与版本。
pub const ARCHIVE_FORMAT: &str = "thought-forge.archive";
pub const ARCHIVE_FORMAT_VERSION: i64 = 1;

/// 清除支持的档案范围，目前只有全量。
pub const SCOPE_ALL: &str = "all";

/// 业务表清单与展示名。顺序即清除顺序：先删子表再删父表，避免外键冲突。
pub const DATA_TABLES: &[(&str, &str)] = &[
    ("corpus_citations", "语料引用"),
    ("corpus_search", "语料检索索引"),
    ("corpus_items", "语料登记"),
    ("master_units", "大师技能单元"),
    ("master_versions", "大师版本快照"),
    ("masters", "大师"),
    ("council_turns", "会诊发言"),
    ("council_round_metrics", "会诊轮次指标"),
    ("council_sources", "检索快照"),
    ("council_panels", "会诊阵容"),
    ("council_sessions", "会诊会话"),
    ("connector_calls", "连接器调用审计"),
    ("connectors", "连接器配置"),
    ("master_pairings", "大师对立度"),
    ("llm_calls", "模型调用审计"),
    ("ai_platforms", "模型平台配置"),
    ("thought_edges", "思维网络连线"),
    ("node_activations", "节点激活"),
    ("thought_clusters", "认知社区"),
    ("thought_nodes", "思维网络节点"),
    ("thought_records", "思考记录"),
    ("consolidation_runs", "固化报告"),
    ("insights", "洞察"),
    ("companion_settings", "主动助学设置"),
    ("signals", "入库信号"),
    ("intake_jobs", "入库任务"),
    ("distill_jobs", "蒸馏任务"),
    ("discovery_settings", "主动搜集设置"),
    ("capture_summaries", "采集摘要"),
    ("capture_events", "采集记录"),
    ("capture_audit", "采集审计"),
    ("capture_settings", "采集能力开关"),
    ("kb_search", "知识库检索索引"),
    ("kb_documents", "知识库文档"),
    ("kb_topics", "知识库主题"),
    ("kb_sources", "知识库来源"),
    ("self_items", "自我蒸馏条目"),
    ("self_drafts", "自我蒸馏草稿"),
    ("skill_dependencies", "Skill 依赖"),
    ("skills", "Skill 索引"),
    ("asset_scans", "资产扫描记录"),
    ("asset_roots", "Skill 根目录"),
    ("data_events", "数据操作审计"),
    ("cost_days", "成本日汇总"),
    ("credential_refs", "凭据引用"),
    ("backups", "备份留痕"),
    ("app_meta", "应用元数据"),
    ("settings", "设置"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataTableCount {
    pub table: String,
    pub label: String,
    pub rows: i64,
}

/// 导出/清除的范围概览。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataScope {
    pub tables: Vec<DataTableCount>,
    pub table_count: i64,
    pub row_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    pub path: String,
    pub table_count: i64,
    pub row_count: i64,
    pub bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeOutcome {
    pub scope: String,
    pub table_count: i64,
    pub row_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEventView {
    pub id: String,
    pub kind: String,
    pub kind_label: String,
    pub scope: String,
    pub table_count: i64,
    pub row_count: i64,
    pub location: String,
    pub created_at: String,
}
