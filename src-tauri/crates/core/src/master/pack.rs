//! 大师包格式与校验。
//!
//! 一个大师包是一个目录，至少包含 `master.json`；语料以 `ref` 指向包内相对路径
//! 或包外绝对路径，只登记元数据，不复制正文。

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{CoreError, CoreResult};
use crate::master::Layer;

pub const PACK_FORMAT: &str = "thought-forge.master-pack";
pub const PACK_FORMAT_VERSION: i64 = 1;

const MAX_ID_LEN: usize = 63;
const MAX_NAME_LEN: usize = 64;
const MAX_DOMAIN_LEN: usize = 32;
const MAX_UNITS: usize = 400;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawPack {
    format: String,
    format_version: i64,
    id: String,
    name: String,
    domain: String,
    layers: Vec<String>,
    version: i64,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    style: String,
    #[serde(default)]
    blind_spots: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    units: Vec<RawUnit>,
    #[serde(default)]
    corpus: Vec<RawCorpus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawUnit {
    title: String,
    layer: String,
    trigger_condition: String,
    steps: Vec<String>,
    mechanism: String,
    boundary: String,
    #[serde(default)]
    evidence: Vec<RawEvidence>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawEvidence {
    corpus_ref: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    location: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawCorpus {
    #[serde(rename = "ref")]
    reference: String,
    kind: String,
    title: String,
    #[serde(default)]
    location_hint: String,
}

/// 校验通过后的大师包，语料路径已解析、内容指纹已计算。
#[derive(Debug, Clone)]
pub struct ValidatedPack {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub layers: Vec<Layer>,
    pub version: i64,
    pub summary: String,
    pub style: String,
    pub blind_spots: String,
    pub note: String,
    pub units: Vec<ValidatedUnit>,
    pub corpus: Vec<ValidatedCorpus>,
}

#[derive(Debug, Clone)]
pub struct ValidatedUnit {
    pub title: String,
    pub layer: Layer,
    pub trigger_condition: String,
    pub steps: Vec<String>,
    pub mechanism: String,
    pub boundary: String,
    pub evidence: Vec<ValidatedEvidence>,
}

#[derive(Debug, Clone)]
pub struct ValidatedEvidence {
    pub corpus_ref: String,
    pub excerpt: String,
    pub location: String,
}

#[derive(Debug, Clone)]
pub struct ValidatedCorpus {
    pub reference: String,
    pub kind: String,
    pub title: String,
    pub location_hint: String,
    pub resolved_path: PathBuf,
    pub content_hash: String,
    pub byte_size: u64,
}

/// 读取并校验大师包目录。任何未通过项都会汇总后一次性报出，便于一次改完。
pub fn load_and_validate(pack_dir: &Path) -> CoreResult<ValidatedPack> {
    let manifest = pack_dir.join("master.json");
    if !manifest.is_file() {
        return Err(CoreError::PackInvalid(vec![format!(
            "缺少 master.json：{}",
            manifest.display()
        )]));
    }

    let text = std::fs::read_to_string(&manifest)?;
    let raw: RawPack = serde_json::from_str(&text).map_err(|error| {
        CoreError::PackInvalid(vec![format!("master.json 不是合法的大师包结构：{error}")])
    })?;

    validate(raw, pack_dir)
}

fn validate(raw: RawPack, pack_dir: &Path) -> CoreResult<ValidatedPack> {
    let mut issues = Vec::new();

    if raw.format != PACK_FORMAT {
        issues.push(format!(
            "format 应为 {PACK_FORMAT}，实际为 {}",
            raw.format
        ));
    }
    if raw.format_version != PACK_FORMAT_VERSION {
        issues.push(format!(
            "formatVersion 应为 {PACK_FORMAT_VERSION}，实际为 {}",
            raw.format_version
        ));
    }
    if !is_valid_id(&raw.id) {
        issues.push(format!(
            "id 只能由小写字母、数字与连字符组成，且不超过 {MAX_ID_LEN} 字符，实际为 {}",
            raw.id
        ));
    }
    if raw.name.trim().is_empty() || raw.name.chars().count() > MAX_NAME_LEN {
        issues.push(format!("name 不能为空且不超过 {MAX_NAME_LEN} 字"));
    }
    if raw.domain.trim().is_empty() || raw.domain.chars().count() > MAX_DOMAIN_LEN {
        issues.push(format!("domain 不能为空且不超过 {MAX_DOMAIN_LEN} 字"));
    }
    if raw.version < 1 {
        issues.push("version 必须为不小于 1 的整数".to_string());
    }

    let mut layers: Vec<Layer> = Vec::new();
    if raw.layers.is_empty() {
        issues.push("layers 至少需要一个层次".to_string());
    }
    for value in &raw.layers {
        match Layer::parse(value) {
            Some(layer) => {
                if layers.contains(&layer) {
                    issues.push(format!("layers 中层次重复：{value}"));
                } else {
                    layers.push(layer);
                }
            }
            None => issues.push(format!(
                "层次取值不合法：{value}（可选 dao/fa/shu/qi/tool/shi）"
            )),
        }
    }
    layers.sort();

    if raw.units.is_empty() {
        issues.push("units 不能为空，一位大师至少需要一个技能单元".to_string());
    }
    if raw.units.len() > MAX_UNITS {
        issues.push(format!("units 数量超过上限 {MAX_UNITS}"));
    }

    let mut corpus = Vec::new();
    let mut refs = Vec::new();
    for entry in &raw.corpus {
        if entry.reference.trim().is_empty() {
            issues.push("corpus 的 ref 不能为空".to_string());
            continue;
        }
        if refs.contains(&entry.reference) {
            issues.push(format!("corpus 的 ref 重复：{}", entry.reference));
            continue;
        }
        if entry.kind.trim().is_empty() {
            issues.push(format!("corpus[{}] 的 kind 不能为空", entry.reference));
        }
        if entry.title.trim().is_empty() {
            issues.push(format!("corpus[{}] 的 title 不能为空", entry.reference));
        }
        match resolve_corpus(pack_dir, &entry.reference) {
            Ok((path, content_hash, byte_size)) => {
                refs.push(entry.reference.clone());
                corpus.push(ValidatedCorpus {
                    reference: entry.reference.clone(),
                    kind: entry.kind.trim().to_string(),
                    title: entry.title.trim().to_string(),
                    location_hint: entry.location_hint.trim().to_string(),
                    resolved_path: path,
                    content_hash,
                    byte_size,
                });
            }
            Err(message) => issues.push(message),
        }
    }

    let mut titles: Vec<String> = Vec::new();
    let mut units = Vec::new();
    for (index, unit) in raw.units.iter().enumerate() {
        let title = unit.title.trim().to_string();
        if title.is_empty() {
            issues.push(format!("units[{index}] 的 title 不能为空"));
        } else if titles.contains(&title) {
            issues.push(format!("技能单元标题重复：{title}"));
        } else {
            titles.push(title.clone());
        }

        let layer = match Layer::parse(unit.layer.trim()) {
            Some(layer) => Some(layer),
            None => {
                issues.push(format!(
                    "技能单元「{title}」的 layer 取值不合法：{}",
                    unit.layer
                ));
                None
            }
        };
        if let Some(layer) = layer {
            if !layers.contains(&layer) {
                issues.push(format!(
                    "技能单元「{title}」的层次 {} 不在大师声明的 layers 内",
                    layer.name()
                ));
            }
        }

        // 四要素：触发条件、步骤、机制、边界。
        if unit.trigger_condition.trim().is_empty() {
            issues.push(format!("技能单元「{title}」缺少触发条件"));
        }
        if unit.mechanism.trim().is_empty() {
            issues.push(format!("技能单元「{title}」缺少作用机制"));
        }
        if unit.boundary.trim().is_empty() {
            issues.push(format!("技能单元「{title}」缺少适用边界"));
        }
        let steps: Vec<String> = unit
            .steps
            .iter()
            .map(|step| step.trim().to_string())
            .filter(|step| !step.is_empty())
            .collect();
        if steps.is_empty() {
            issues.push(format!("技能单元「{title}」缺少执行步骤"));
        } else if steps.len() != unit.steps.len() {
            issues.push(format!("技能单元「{title}」的执行步骤中存在空项"));
        }

        if unit.evidence.is_empty() {
            issues.push(format!("技能单元「{title}」缺少来源标注"));
        }
        let mut evidence = Vec::new();
        for (position, item) in unit.evidence.iter().enumerate() {
            let reference = item.corpus_ref.trim();
            if reference.is_empty() {
                issues.push(format!("技能单元「{title}」第 {} 条来源缺少 corpusRef", position + 1));
                continue;
            }
            if !refs.contains(&reference.to_string()) {
                issues.push(format!(
                    "技能单元「{title}」的来源指向未声明的语料：{reference}"
                ));
            }
            if item.excerpt.trim().is_empty() && item.location.trim().is_empty() {
                issues.push(format!(
                    "技能单元「{title}」第 {} 条来源需要摘录或位置至少其一",
                    position + 1
                ));
            }
            evidence.push(ValidatedEvidence {
                corpus_ref: reference.to_string(),
                excerpt: item.excerpt.trim().to_string(),
                location: item.location.trim().to_string(),
            });
        }

        if title.is_empty() {
            continue;
        }
        units.push(ValidatedUnit {
            title,
            layer: layer.unwrap_or(Layer::Fa),
            trigger_condition: unit.trigger_condition.trim().to_string(),
            steps,
            mechanism: unit.mechanism.trim().to_string(),
            boundary: unit.boundary.trim().to_string(),
            evidence,
        });
    }

    if !issues.is_empty() {
        return Err(CoreError::PackInvalid(issues));
    }

    Ok(ValidatedPack {
        id: raw.id,
        name: raw.name.trim().to_string(),
        domain: raw.domain.trim().to_string(),
        layers,
        version: raw.version,
        summary: raw.summary.trim().to_string(),
        style: raw.style.trim().to_string(),
        blind_spots: raw.blind_spots.trim().to_string(),
        note: raw.note.trim().to_string(),
        units,
        corpus,
    })
}

fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    if id.len() > MAX_ID_LEN {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// 语料优先按包内相对路径查找，找不到再尝试包外绝对路径。
fn resolve_corpus(pack_dir: &Path, reference: &str) -> Result<(PathBuf, String, u64), String> {
    let inside = pack_dir.join(reference);
    let candidate = if inside.is_file() {
        inside
    } else {
        let external = PathBuf::from(reference);
        if external.is_absolute() && external.is_file() {
            external
        } else {
            return Err(format!("语料文件不存在：{reference}"));
        }
    };

    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("语料路径无法解析：{reference}（{error}）"))?;
    let (hash, size) = hash_file(&canonical).map_err(|error| format!("读取语料失败：{reference}（{error}）"))?;
    Ok((canonical, hash, size))
}

/// 流式计算指纹，避免把整本书读进内存。
fn hash_file(path: &Path) -> CoreResult<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}
