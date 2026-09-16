//! 资产统计：本机 Skill 目录的索引、分类统计与依赖登记。
//!
//! 扫描只读取 Skill 的清单文件（优先 `manifest.json`，其次 `SKILL.md` 的
//! YAML 头），不读 Skill 正文。清单缺失或格式无效时保留记录并标记
//! `needs_repair`，让用户能看到并修好它，而不是让它从统计里消失。

use serde::Serialize;

pub mod repo;
pub mod service;

/// 清单文件名优先级。
pub const MANIFEST_JSON: &str = "manifest.json";
pub const MANIFEST_MARKDOWN: &str = "SKILL.md";

pub const DEFAULT_SKILL_LIMIT: i64 = 200;
pub const MAX_SKILL_LIMIT: i64 = 1000;

/// 「近 30 天变动」的窗口。
pub const RECENT_WINDOW_DAYS: i64 = 30;

/// 未归类 Skill 在统计里的展示名。
pub const UNCATEGORIZED: &str = "未归类";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRootView {
    pub id: String,
    pub path: String,
    pub available: bool,
    pub last_scan_at: Option<String>,
    pub skill_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillView {
    pub id: String,
    pub root_id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub source: String,
    pub path: String,
    pub version: String,
    pub needs_repair: bool,
    pub repair_reason: String,
    pub missing: bool,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDependencyView {
    pub name: String,
    pub version: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub skill: SkillView,
    pub dependencies: Vec<SkillDependencyView>,
    /// 清单原文。文件已不可读时为空串。
    pub manifest_excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStat {
    pub category: String,
    pub skill_count: i64,
    pub enabled_count: i64,
}

/// 已接入模型平台的接入状态，供资产总览汇总。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStat {
    pub code: String,
    pub display_name: String,
    pub model_name: String,
    pub enabled: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSummary {
    pub root_count: i64,
    pub available_roots: i64,
    pub skill_count: i64,
    pub enabled_count: i64,
    pub disabled_count: i64,
    pub needs_repair_count: i64,
    pub recent_added: i64,
    pub recent_removed: i64,
    pub categories: Vec<CategoryStat>,
    pub platforms: Vec<PlatformStat>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetScanOutcome {
    pub root_id: String,
    pub root_path: String,
    pub available: bool,
    pub scanned: i64,
    pub added: i64,
    pub updated: i64,
    pub removed: i64,
    pub needs_repair: i64,
    pub reason: Option<String>,
}

/// Skill 筛选条件。
#[derive(Debug, Clone, Default)]
pub struct SkillFilter {
    pub root_id: Option<String>,
    pub category: Option<String>,
    pub tag: Option<String>,
    pub enabled: Option<bool>,
    pub needs_repair: Option<bool>,
    pub query: Option<String>,
    pub limit: Option<i64>,
}

/// 从清单解析出的 Skill 元数据。
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub version: String,
    pub dependencies: Vec<ManifestDependency>,
    /// 命中的清单文件名；为空表示两者都没有。
    pub source: String,
    pub needs_repair: bool,
    pub repair_reason: String,
}

#[derive(Debug, Clone)]
pub struct ManifestDependency {
    pub name: String,
    pub version: String,
}

/// 解析一个 Skill 目录的清单。缺失或无效时返回带原因的 `needs_repair` 结果。
pub fn parse_manifest(dir: &std::path::Path, name_hint: &str) -> Manifest {
    let json_path = dir.join(MANIFEST_JSON);
    if let Ok(text) = std::fs::read_to_string(&json_path) {
        return from_json(&text, name_hint);
    }
    let md_path = dir.join(MANIFEST_MARKDOWN);
    if let Ok(text) = std::fs::read_to_string(&md_path) {
        return from_markdown(&text, name_hint);
    }
    Manifest {
        name: name_hint.to_string(),
        enabled: true,
        needs_repair: true,
        repair_reason: "manifest_missing".to_string(),
        ..Manifest::default()
    }
}

/// 读取清单原文，供详情展示。文件不可读时返回空串。
pub fn manifest_excerpt(dir: &std::path::Path, source: &str) -> String {
    let file = if source.is_empty() {
        let json_path = dir.join(MANIFEST_JSON);
        if json_path.is_file() {
            json_path
        } else {
            dir.join(MANIFEST_MARKDOWN)
        }
    } else {
        dir.join(source)
    };
    std::fs::read_to_string(file).unwrap_or_default()
}

fn from_json(text: &str, name_hint: &str) -> Manifest {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return Manifest {
                name: name_hint.to_string(),
                enabled: true,
                needs_repair: true,
                repair_reason: format!("manifest_invalid: {error}"),
                source: MANIFEST_JSON.to_string(),
                ..Manifest::default()
            }
        }
    };
    let name = string_field(&value, "name")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| name_hint.to_string());
    let needs_repair = string_field(&value, "name").is_none_or(|value| value.is_empty());
    Manifest {
        name,
        description: string_field(&value, "description").unwrap_or_default(),
        category: string_field(&value, "category").unwrap_or_default(),
        tags: tags_from_json(&value),
        enabled: value.get("enabled").and_then(|tag| tag.as_bool()).unwrap_or(true),
        version: string_field(&value, "version").unwrap_or_default(),
        dependencies: dependencies_from_json(&value),
        source: MANIFEST_JSON.to_string(),
        needs_repair,
        repair_reason: if needs_repair {
            "name_missing".to_string()
        } else {
            String::new()
        },
    }
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|tag| tag.as_str())
        .map(|text| text.trim().to_string())
}

fn tags_from_json(value: &serde_json::Value) -> Vec<String> {
    value
        .get("tags")
        .and_then(|tag| tag.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn dependencies_from_json(value: &serde_json::Value) -> Vec<ManifestDependency> {
    let Some(items) = value.get("dependencies").and_then(|tag| tag.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            serde_json::Value::String(name) => Some(ManifestDependency {
                name: name.trim().to_string(),
                version: String::new(),
            }),
            serde_json::Value::Object(_) => Some(ManifestDependency {
                name: string_field(item, "name").unwrap_or_default(),
                version: string_field(item, "version").unwrap_or_default(),
            }),
            _ => None,
        })
        .filter(|dependency| !dependency.name.is_empty())
        .collect()
}

/// 解析 `SKILL.md` 的 YAML 头。只支持清单需要的扁平子集：
/// 标量、内联列表与缩进短横线列表。头缺失或没有 name 时标记待修复。
fn from_markdown(text: &str, name_hint: &str) -> Manifest {
    let Some(head) = frontmatter(text) else {
        return Manifest {
            name: name_hint.to_string(),
            enabled: true,
            needs_repair: true,
            repair_reason: "frontmatter_missing".to_string(),
            source: MANIFEST_MARKDOWN.to_string(),
            ..Manifest::default()
        };
    };
    let fields = parse_yaml_subset(&head);
    let name = fields
        .iter()
        .find(|(key, _)| key == "name")
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| name_hint.to_string());
    let needs_repair = fields
        .iter()
        .find(|(key, _)| key == "name")
        .is_none_or(|(_, value)| value.is_empty());
    Manifest {
        name,
        description: fields
            .iter()
            .find(|(key, _)| key == "description")
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        category: fields
            .iter()
            .find(|(key, _)| key == "category")
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        tags: list_field(&fields, "tags"),
        enabled: fields
            .iter()
            .find(|(key, _)| key == "enabled")
            .is_none_or(|(_, value)| !value.eq_ignore_ascii_case("false")),
        version: fields
            .iter()
            .find(|(key, _)| key == "version")
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        dependencies: list_field(&fields, "dependencies")
            .into_iter()
            .map(|name| ManifestDependency {
                name,
                version: String::new(),
            })
            .collect(),
        source: MANIFEST_MARKDOWN.to_string(),
        needs_repair,
        repair_reason: if needs_repair {
            "name_missing".to_string()
        } else {
            String::new()
        },
    }
}

/// 取首个 `---` 与下一个 `---` 之间的内容。
fn frontmatter(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut body = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(body.join("\n"));
        }
        body.push(line);
    }
    None
}

/// 极简 YAML 解析：`key: value`、`key: [a, b]` 与缩进 `- item` 列表。
/// 仅覆盖清单字段，不做嵌套与类型推断。
fn parse_yaml_subset(text: &str) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut list_owner: Option<usize> = None;
    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(item) = trimmed.strip_prefix("- ") {
            if let Some(index) = list_owner {
                let value = scalar(item);
                if !value.is_empty() {
                    if !fields[index].1.is_empty() {
                        fields[index].1.push(',');
                    }
                    fields[index].1.push_str(&value);
                }
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        if key.is_empty() {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            fields.push((key, String::new()));
            list_owner = Some(fields.len() - 1);
        } else {
            fields.push((key, inline_list(value)));
            list_owner = None;
        }
    }
    fields
}

/// 内联列表 `[a, b]` 转成逗号分隔；标量去掉引号。
fn inline_list(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(inner) = trimmed.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        return inner
            .split(',')
            .map(scalar)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(",");
    }
    scalar(trimmed)
}

fn scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch| ch == '"' || ch == '\'')
        .trim()
        .to_string()
}

fn list_field(fields: &[(String, String)], key: &str) -> Vec<String> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// 把 tags 存成 JSON 数组。
pub fn tags_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

/// 读回 tags，脏数据按空数组处理。
pub fn tags_from_json_text(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

/// 分类展示名：空分类归入「未归类」。
pub fn category_label(category: &str) -> String {
    let trimmed = category.trim();
    if trimmed.is_empty() {
        UNCATEGORIZED.to_string()
    } else {
        trimmed.to_string()
    }
}
