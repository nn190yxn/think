//! 分层知识库的第二层：原始语料登记、检索与引用溯源。

pub mod repo;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusItemView {
    pub id: String,
    pub source_kind: String,
    pub source_ref: String,
    pub title: String,
    pub normalized_name: String,
    pub location_hint: String,
    pub content_hash: String,
    pub byte_size: i64,
    /// 语料文件当前是否仍可访问。来源缺失时大师包照常可用。
    pub available: bool,
    pub master_ids: Vec<String>,
    pub registered_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusSearchHit {
    pub item: CorpusItemView,
    /// 命中方式：fts 表示全文索引，like 表示短查询回退。
    pub matched_by: String,
}
