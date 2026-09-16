//! P9 资产统计：Skill 目录扫描、清单解析、待修复标记与分类统计。

use std::fs;
use std::path::Path;

use proptest::prelude::*;
use thought_forge_core::asset::service::{self as asset_service};
use thought_forge_core::asset::SkillFilter;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::llm::platform::{self as platform_repo, PlatformInput};

fn db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn write(dir: &Path, relative: &str, body: &str) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

/// 一份典型根目录：json 清单、YAML 头、缺清单、坏清单与应被跳过的项。
fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "writing/manifest.json",
        r#"{
            "name": "写作",
            "description": "把判断写成结构化长文",
            "category": "写作",
            "tags": ["文案", "表达"],
            "enabled": true,
            "version": "1.2",
            "dependencies": [{"name": "typo-check", "version": "0.3"}, "rag"]
        }"#,
    );
    write(
        dir.path(),
        "research/SKILL.md",
        "---\nname: 研究\ndescription: 检索与交叉验证\ncategory: 研究\ntags: [检索, 分析]\nenabled: false\nversion: 2\n---\n\n正文不应被读取\n",
    );
    write(dir.path(), "broken/notes.txt", "没有清单");
    write(dir.path(), "invalid/manifest.json", "{ not json");
    write(dir.path(), ".hidden/manifest.json", r#"{"name": "隐藏"}"#);
    write(dir.path(), "loose.md", "根目录下的散落文件");
    dir
}

fn scan(conn: &mut rusqlite::Connection, root_id: &str) -> thought_forge_core::asset::AssetScanOutcome {
    asset_service::scan_assets(conn, Some(root_id))
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn add_root_is_idempotent_by_path() {
    let dir = setup();
    let conn = db();
    let path = dir.path().to_string_lossy().to_string();
    let first = asset_service::add_root(&conn, &path).unwrap();
    let second = asset_service::add_root(&conn, &path).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(asset_service::list_roots(&conn).unwrap().len(), 1);
}

#[test]
fn empty_path_is_rejected() {
    let conn = db();
    let error = asset_service::add_root(&conn, "   ").unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
}

#[test]
fn scan_indexes_skills_and_skips_hidden_and_files() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    let outcome = scan(&mut conn, &root.id);

    assert!(outcome.available);
    assert_eq!(outcome.scanned, 4, "四个 Skill 目录，隐藏目录与散落文件不计入");
    assert_eq!(outcome.added, 4);
    assert_eq!(outcome.updated, 0);
    assert_eq!(outcome.needs_repair, 2);

    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    assert_eq!(skills.len(), 4);
    assert!(!skills.iter().any(|skill| skill.name == "隐藏"));
}

#[test]
fn json_manifest_fields_are_read() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);

    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    let writing = skills.iter().find(|skill| skill.name == "写作").unwrap();
    assert_eq!(writing.description, "把判断写成结构化长文");
    assert_eq!(writing.category, "写作");
    assert_eq!(writing.tags, vec!["文案", "表达"]);
    assert!(writing.enabled);
    assert_eq!(writing.version, "1.2");
    assert_eq!(writing.source, "manifest.json");
    assert!(!writing.needs_repair);
}

#[test]
fn markdown_frontmatter_is_read_as_fallback() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);

    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    let research = skills.iter().find(|skill| skill.name == "研究").unwrap();
    assert_eq!(research.category, "研究");
    assert_eq!(research.tags, vec!["检索", "分析"]);
    assert!(!research.enabled, "YAML 头里的 enabled: false 应生效");
    assert_eq!(research.version, "2");
    assert_eq!(research.source, "SKILL.md");
    assert!(!research.needs_repair);
}

#[test]
fn missing_and_invalid_manifests_are_retained_for_repair() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);

    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    let broken = skills.iter().find(|skill| skill.path.ends_with("broken")).unwrap();
    assert!(broken.needs_repair);
    assert_eq!(broken.repair_reason, "manifest_missing");
    assert_eq!(broken.name, "broken", "缺清单时以目录名兜底");

    let invalid = skills.iter().find(|skill| skill.path.ends_with("invalid")).unwrap();
    assert!(invalid.needs_repair);
    assert!(invalid.repair_reason.starts_with("manifest_invalid"));
}

#[test]
fn detail_returns_dependencies_and_manifest_excerpt() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);

    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    let writing = skills.iter().find(|skill| skill.name == "写作").unwrap();
    let detail = asset_service::skill_detail(&conn, &writing.id).unwrap();

    assert_eq!(detail.dependencies.len(), 2);
    assert!(detail.dependencies.iter().any(|dep| dep.name == "typo-check" && dep.version == "0.3"));
    assert!(detail.dependencies.iter().any(|dep| dep.name == "rag"));
    assert!(detail.manifest_excerpt.contains("写作"));
}

#[test]
fn rescan_is_idempotent_and_updates_changed_manifest() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    let first = scan(&mut conn, &root.id);
    assert_eq!(first.added, 4);

    let second = scan(&mut conn, &root.id);
    assert_eq!(second.added, 0);
    assert_eq!(second.updated, 4);
    assert_eq!(second.removed, 0);

    // 改清单后重扫：不新增记录，元数据与分类统计跟着变。
    write(
        dir.path(),
        "broken/manifest.json",
        r#"{"name": "补好的", "category": "写作"}"#,
    );
    let third = scan(&mut conn, &root.id);
    assert_eq!(third.added, 0);
    assert_eq!(third.needs_repair, 1);
    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    assert_eq!(skills.len(), 4);
    assert!(skills.iter().any(|skill| skill.name == "补好的"));
}

#[test]
fn removed_skill_directory_is_pruned() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);

    fs::remove_dir_all(dir.path().join("research")).unwrap();
    let outcome = scan(&mut conn, &root.id);
    assert_eq!(outcome.removed, 1);
    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    assert!(!skills.iter().any(|skill| skill.name == "研究"));
}

#[test]
fn offline_root_keeps_skills_but_marks_missing() {
    let dir = setup();
    let mut conn = db();
    let path = dir.path().to_string_lossy().to_string();
    let root = asset_service::add_root(&conn, &path).unwrap();
    scan(&mut conn, &root.id);

    let offline = format!("{path}-offline");
    fs::rename(dir.path(), &offline).unwrap();
    let outcome = scan(&mut conn, &root.id);
    assert!(!outcome.available);
    assert_eq!(outcome.reason.as_deref(), Some("root_unavailable"));
    assert_eq!(outcome.removed, 0);

    let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    assert_eq!(skills.len(), 4, "根目录离线不应删除既有记录");
    assert!(skills.iter().all(|skill| skill.missing));

    fs::rename(&offline, dir.path()).unwrap();
    let recovered = scan(&mut conn, &root.id);
    assert!(recovered.available);
    let restored = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
    assert!(restored.iter().all(|skill| !skill.missing));
}

#[test]
fn summary_reports_categories_ratio_and_recent_changes() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);

    let summary = asset_service::summary(&conn).unwrap();
    assert_eq!(summary.root_count, 1);
    assert_eq!(summary.available_roots, 1);
    assert_eq!(summary.skill_count, 4);
    assert_eq!(summary.enabled_count, 3);
    assert_eq!(summary.disabled_count, 1);
    assert_eq!(summary.needs_repair_count, 2);
    assert_eq!(summary.recent_added, 4);
    assert_eq!(summary.recent_removed, 0);
    assert!(summary.categories.iter().any(|stat| stat.category == "写作" && stat.skill_count == 1));
    assert!(summary
        .categories
        .iter()
        .any(|stat| stat.category == "未归类" && stat.skill_count == 2));
}

#[test]
fn summary_includes_configured_platforms() {
    let conn = db();
    platform_repo::upsert(
        &conn,
        &PlatformInput {
            code: "sensenova".to_string(),
            display_name: "商汤".to_string(),
            endpoint: "https://token.sensenova.cn/v1/chat/completions".to_string(),
            model_name: "sensenova-6.7-flash-lite".to_string(),
            input_price_micros_per_1k: 0,
            output_price_micros_per_1k: 0,
            currency: "CNY".to_string(),
        },
    )
    .unwrap();
    platform_repo::set_enabled(&conn, "sensenova", true).unwrap();

    let summary = asset_service::summary(&conn).unwrap();
    let platform = summary.platforms.iter().find(|item| item.code == "sensenova").unwrap();
    assert_eq!(platform.model_name, "sensenova-6.7-flash-lite");
    assert!(platform.enabled);
    assert_eq!(platform.status, "ready");
}

#[test]
fn filters_narrow_skill_list() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);

    let by_category = asset_service::list_skills(
        &conn,
        &SkillFilter {
            category: Some("研究".to_string()),
            ..SkillFilter::default()
        },
    )
    .unwrap();
    assert_eq!(by_category.len(), 1);

    let disabled = asset_service::list_skills(
        &conn,
        &SkillFilter {
            enabled: Some(false),
            ..SkillFilter::default()
        },
    )
    .unwrap();
    assert_eq!(disabled.len(), 1);

    let by_tag = asset_service::list_skills(
        &conn,
        &SkillFilter {
            tag: Some("文案".to_string()),
            ..SkillFilter::default()
        },
    )
    .unwrap();
    assert_eq!(by_tag.len(), 1);

    let repairing = asset_service::list_skills(
        &conn,
        &SkillFilter {
            needs_repair: Some(true),
            ..SkillFilter::default()
        },
    )
    .unwrap();
    assert_eq!(repairing.len(), 2);

    let by_query = asset_service::list_skills(
        &conn,
        &SkillFilter {
            query: Some("交叉验证".to_string()),
            ..SkillFilter::default()
        },
    )
    .unwrap();
    assert_eq!(by_query.len(), 1);
}

#[test]
fn remove_root_drops_skills() {
    let dir = setup();
    let mut conn = db();
    let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &root.id);
    assert_eq!(asset_service::summary(&conn).unwrap().skill_count, 4);

    assert!(asset_service::remove_root(&conn, &root.id).unwrap());
    let summary = asset_service::summary(&conn).unwrap();
    assert_eq!(summary.root_count, 0);
    assert_eq!(summary.skill_count, 0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// 属性：重复扫描不新增 Skill，索引规模稳定。
    #[test]
    fn property_rescan_is_stable(extra in 0usize..4) {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..extra {
            write(
                dir.path(),
                &format!("skill{index}/manifest.json"),
                &format!(r#"{{"name": "技能{index}", "category": "测试"}}"#),
            );
        }
        let mut conn = db();
        let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
        let first = asset_service::scan_assets(&mut conn, Some(&root.id)).unwrap();
        prop_assert_eq!(first[0].added as usize, extra);

        for _ in 0..3 {
            let again = asset_service::scan_assets(&mut conn, Some(&root.id)).unwrap();
            prop_assert_eq!(again[0].added, 0);
            prop_assert_eq!(again[0].removed, 0);
        }
        prop_assert_eq!(
            asset_service::list_skills(&conn, &SkillFilter::default()).unwrap().len(),
            extra
        );
    }

    /// 属性：清单缺失的 Skill 一定被保留并标记待修复。
    #[test]
    fn property_missing_manifest_is_retained(count in 1usize..4) {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..count {
            write(dir.path(), &format!("broken{index}/readme.txt"), "没有清单");
        }
        let mut conn = db();
        let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
        let outcome = asset_service::scan_assets(&mut conn, Some(&root.id)).unwrap();
        prop_assert_eq!(outcome[0].needs_repair as usize, count);

        let skills = asset_service::list_skills(&conn, &SkillFilter::default()).unwrap();
        prop_assert_eq!(skills.len(), count);
        prop_assert!(skills.iter().all(|skill| skill.needs_repair));
    }

    /// 属性：启用数量不会超过 Skill 总数，停用数量是其补集。
    #[test]
    fn property_enabled_ratio_is_consistent(enabled_flags in prop::collection::vec(any::<bool>(), 1..4)) {
        let dir = tempfile::tempdir().unwrap();
        for (index, enabled) in enabled_flags.iter().enumerate() {
            write(
                dir.path(),
                &format!("skill{index}/manifest.json"),
                &format!(r#"{{"name": "技能{index}", "enabled": {enabled}}}"#),
            );
        }
        let mut conn = db();
        let root = asset_service::add_root(&conn, &dir.path().to_string_lossy()).unwrap();
        asset_service::scan_assets(&mut conn, Some(&root.id)).unwrap();

        let summary = asset_service::summary(&conn).unwrap();
        prop_assert!(summary.enabled_count <= summary.skill_count);
        prop_assert_eq!(summary.skill_count, enabled_flags.len() as i64);
        prop_assert_eq!(
            summary.enabled_count,
            enabled_flags.iter().filter(|flag| **flag).count() as i64
        );
        prop_assert_eq!(summary.disabled_count, summary.skill_count - summary.enabled_count);
    }
}
