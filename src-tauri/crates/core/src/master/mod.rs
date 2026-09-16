//! 大师：层次模型、大师包格式、校验与仓储。

pub mod pack;
pub mod repo;

use serde::{Deserialize, Serialize};

/// 六个层次。顺序即从抽象到具体再到期势，展示与覆盖矩阵都按此顺序。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Dao,
    Fa,
    Shu,
    Qi,
    Tool,
    Shi,
}

pub const LAYER_ORDER: [Layer; 6] = [
    Layer::Dao,
    Layer::Fa,
    Layer::Shu,
    Layer::Qi,
    Layer::Tool,
    Layer::Shi,
];

impl Layer {
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Dao => "dao",
            Layer::Fa => "fa",
            Layer::Shu => "shu",
            Layer::Qi => "qi",
            Layer::Tool => "tool",
            Layer::Shi => "shi",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Layer::Dao => "道",
            Layer::Fa => "法",
            Layer::Shu => "术",
            Layer::Qi => "气",
            Layer::Tool => "器",
            Layer::Shi => "势",
        }
    }

    pub fn parse(value: &str) -> Option<Layer> {
        LAYER_ORDER.iter().copied().find(|layer| layer.as_str() == value)
    }
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// 技能单元：判断框架的最小可调用单位，四要素缺一不可。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterUnitView {
    pub id: String,
    pub title: String,
    pub layer: Layer,
    pub trigger_condition: String,
    pub steps: Vec<String>,
    pub mechanism: String,
    pub boundary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flagged_reason: Option<String>,
    pub citations: Vec<CitationView>,
}

/// 引用溯源：技能单元指向原始语料的具体位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corpus_item_id: Option<String>,
    pub excerpt: String,
    pub location: String,
    /// 语料文件当前是否仍可访问，用于标注来源缺失。
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterSummary {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub layers: Vec<Layer>,
    pub status: String,
    pub current_version: i64,
    pub unit_count: i64,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDiff {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub carried: usize,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionView {
    pub version: i64,
    pub unit_count: i64,
    pub note: String,
    pub created_at: String,
    pub diff: VersionDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterDetail {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub layers: Vec<Layer>,
    pub status: String,
    pub current_version: i64,
    pub summary: String,
    pub style: String,
    pub blind_spots: String,
    pub units: Vec<MasterUnitView>,
    pub versions: Vec<VersionView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub master_id: String,
    pub version: i64,
    pub unit_count: i64,
    pub corpus_count: i64,
    /// 首次安装为 true，增量更新为 false。
    pub created: bool,
    pub diff: VersionDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerCoverage {
    pub layer: Layer,
    pub name: String,
    pub master_count: i64,
    pub unit_count: i64,
    pub masters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainCoverage {
    pub domain: String,
    pub master_count: i64,
    pub present_layers: Vec<Layer>,
    pub missing_layers: Vec<Layer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageMatrix {
    pub layers: Vec<LayerCoverage>,
    pub domains: Vec<DomainCoverage>,
    /// 层次整体空缺时给出的补充建议。
    pub suggestions: Vec<String>,
    pub master_count: i64,
}
