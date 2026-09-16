//! 知识库：只读元数据的本地索引、主题归组与知识地形统计。
//!
//! 扫描过程不读取文件正文，只登记路径、大小、时间与规范化名称，并把文档
//! 归入主题。来源离线时保留既有索引，仅标记不可用。

use serde::Serialize;

pub mod repo;
pub mod service;

/// 纳入索引的文件扩展名。与 `document-index` 的做法一致，只认文档类。
pub const SUPPORTED_EXTENSIONS: [&str; 10] =
    ["md", "txt", "pdf", "doc", "docx", "ppt", "pptx", "xls", "xlsx", "epub"];

/// 年轮展示的月份数。
pub const RING_MONTHS: usize = 12;

pub const DEFAULT_DOC_LIMIT: i64 = 200;
pub const MAX_DOC_LIMIT: i64 = 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbSourceView {
    pub id: String,
    pub path: String,
    pub available: bool,
    pub paused: bool,
    pub last_scan_at: Option<String>,
    pub last_success_at: Option<String>,
    pub doc_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbDocumentView {
    pub id: String,
    pub source_id: String,
    pub path: String,
    pub normalized_name: String,
    pub version_label: String,
    pub domain: String,
    pub topic_id: Option<String>,
    pub topic_name: String,
    pub file_size: i64,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbTopicView {
    pub id: String,
    pub display_name: String,
    pub doc_count: i64,
    pub latest_modified_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbScanOutcome {
    pub source_id: String,
    pub available: bool,
    pub scanned: i64,
    pub added: i64,
    pub updated: i64,
    pub removed: i64,
    pub skipped: i64,
    pub reason: Option<String>,
}

/// 文档筛选条件。
#[derive(Debug, Clone, Default)]
pub struct KbFilter {
    pub source_id: Option<String>,
    pub domain: Option<String>,
    pub topic_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbSearchHit {
    pub document: KbDocumentView,
    pub matched_by: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbDomainStat {
    pub domain: String,
    pub doc_count: i64,
}

/// 年轮的一圈：某个月新增量与累计量。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbRingStat {
    pub period: String,
    pub added: i64,
    pub total: i64,
}

/// 知识地形总览。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeOverview {
    pub source_count: i64,
    pub available_sources: i64,
    pub doc_count: i64,
    pub topic_count: i64,
    pub domains: Vec<KbDomainStat>,
    pub topics: Vec<KbTopicView>,
    pub rings: Vec<KbRingStat>,
}

/// 从文件名解析原标题：去掉扩展名并修剪空白。
pub fn display_stem(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(index) if index > 0 => file_name[..index].trim().to_string(),
        _ => file_name.trim().to_string(),
    }
}

/// 版本标签识别：`v2`、`(2)`、`第 2 版` 与尾部四位年份。
pub fn version_label(stem: &str) -> Option<String> {
    let trimmed = stem.trim_end();
    if let Some(rest) = trimmed.strip_suffix('版') {
        if let Some(index) = rest.rfind('第') {
            let digits: String = rest[index + '第'.len_utf8()..]
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return Some(format!("第{digits}版"));
            }
        }
    }
    if let Some(rest) = trimmed.strip_suffix(')') {
        if let Some(index) = rest.rfind('(') {
            let inner = rest[index + 1..].trim();
            if !inner.is_empty() && inner.chars().all(|ch| ch.is_ascii_digit()) {
                return Some(format!("({inner})"));
            }
        }
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(index) = lower.rfind('v') {
        let suffix: String = trimmed[index + 1..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if !suffix.is_empty() && index > 0 {
            return Some(format!("v{suffix}"));
        }
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() >= 5 {
        let tail: String = chars[chars.len() - 4..].iter().collect();
        let prefix: String = chars[..chars.len() - 4].iter().collect();
        let prefix = prefix.trim_end_matches(['-', '_', ' ']);
        if tail.chars().all(|ch| ch.is_ascii_digit())
            && !prefix.is_empty()
            && (tail.starts_with("19") || tail.starts_with("20"))
        {
            return Some(tail);
        }
    }
    None
}

/// 去掉版本标签后的主题键。
pub fn topic_key(stem: &str, label: Option<&str>) -> String {
    match label {
        Some(label) => stem
            .trim_end()
            .strip_suffix(label)
            .map(|value| value.trim_end_matches(['-', '_', ' ', '·']))
            .filter(|value| !value.is_empty())
            .unwrap_or(stem)
            .trim()
            .to_string(),
        None => stem.trim().to_string(),
    }
}
