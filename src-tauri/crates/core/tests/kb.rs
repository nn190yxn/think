//! P7 知识地形：只读元数据扫描、主题归组、离线保留、检索与年轮统计。

use std::fs;
use std::path::Path;

use proptest::prelude::*;
use tempfile::TempDir;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::kb::service::{self as kb_service};
use thought_forge_core::kb::KbFilter;

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

fn setup() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "注意力经济.md", "正文不应被读取");
    write(dir.path(), "设计文档 v1.md", "第一版");
    write(dir.path(), "设计文档 v2.md", "第二版");
    write(dir.path(), "readme.txt", "普通文本");
    write(dir.path(), "ignore.bin", "不支持的类型");
    write(dir.path(), ".hidden.md", "隐藏文件应被跳过");
    write(dir.path(), "项目/方案.md", "子目录里的文档");
    dir
}

fn scan(conn: &mut rusqlite::Connection, source_id: &str) -> thought_forge_core::kb::KbScanOutcome {
    kb_service::scan_source(conn, source_id).unwrap()
}

#[test]
fn add_source_is_idempotent_by_path() {
    let dir = setup();
    let conn = db();
    let path = dir.path().to_string_lossy().to_string();
    let first = kb_service::add_source(&conn, &path).unwrap();
    let second = kb_service::add_source(&conn, &path).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(kb_service::list_sources(&conn).unwrap().len(), 1);
}

#[test]
fn empty_path_is_rejected() {
    let conn = db();
    let error = kb_service::add_source(&conn, "   ").unwrap_err();
    assert_eq!(error.code(), "E_INVALID_INPUT");
}

#[test]
fn scan_indexes_supported_files_only() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    let outcome = scan(&mut conn, &source.id);

    assert!(outcome.available);
    assert_eq!(outcome.scanned, 5, "md/txt 计入，bin 与隐藏文件不计入");
    assert_eq!(outcome.added, 5);
    assert_eq!(outcome.updated, 0);

    let docs = kb_service::list_documents(&conn, &KbFilter::default()).unwrap();
    let names: Vec<String> = docs.iter().map(|doc| doc.normalized_name.clone()).collect();
    assert!(!names.iter().any(|name| name.contains("ignore")));
    assert!(!names.iter().any(|name| name.contains("hidden")));
}

#[test]
fn scan_groups_versions_under_one_topic_and_records_domain() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &source.id);

    let docs = kb_service::list_documents(&conn, &KbFilter::default()).unwrap();
    let design: Vec<_> = docs
        .iter()
        .filter(|doc| doc.normalized_name.contains("设计文档"))
        .collect();
    assert_eq!(design.len(), 2);
    assert_eq!(design[0].topic_name, "设计文档");
    assert_eq!(design[1].topic_name, "设计文档");
    assert!(design.iter().any(|doc| doc.version_label == "v1"));
    assert!(design.iter().any(|doc| doc.version_label == "v2"));

    let nested = docs
        .iter()
        .find(|doc| doc.normalized_name.contains("方案"))
        .unwrap();
    assert_eq!(nested.domain, "项目");
}

#[test]
fn rescan_is_idempotent_and_updates_changed_files() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    let first = scan(&mut conn, &source.id);
    assert_eq!(first.added, 5);

    let second = scan(&mut conn, &source.id);
    assert_eq!(second.added, 0);
    assert_eq!(second.updated, 5);
    assert_eq!(second.removed, 0);
    assert_eq!(kb_service::list_documents(&conn, &KbFilter::default()).unwrap().len(), 5);
}

#[test]
fn removed_files_are_pruned_from_index() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &source.id);

    // 把文件移出来源目录（移动而非删除），再扫描应将其从索引移除。
    let moved = dir.path().join("readme.txt");
    fs::rename(&moved, dir.path().join("readme.txt.bak")).unwrap();
    let outcome = scan(&mut conn, &source.id);
    assert_eq!(outcome.removed, 1);
    let docs = kb_service::list_documents(&conn, &KbFilter::default()).unwrap();
    assert!(!docs.iter().any(|doc| doc.path.contains("readme.txt")));
}

#[test]
fn offline_source_keeps_index_but_marks_unavailable() {
    let dir = setup();
    let mut conn = db();
    let path = dir.path().to_string_lossy().to_string();
    let source = kb_service::add_source(&conn, &path).unwrap();
    scan(&mut conn, &source.id);
    let before = kb_service::list_documents(&conn, &KbFilter::default()).unwrap();
    assert_eq!(before.len(), 5);

    // 目录改名模拟来源离线。
    let offline = format!("{path}-offline");
    fs::rename(dir.path(), &offline).unwrap();
    let outcome = scan(&mut conn, &source.id);
    assert!(!outcome.available);
    assert_eq!(outcome.reason.as_deref(), Some("source_unavailable"));

    let after = kb_service::list_documents(&conn, &KbFilter::default()).unwrap();
    assert_eq!(after.len(), 5, "离线不应删除既有索引");
    assert!(after.iter().all(|doc| !doc.available));
    assert!(kb_service::list_sources(&conn).unwrap()[0].doc_count == 5);

    // 目录恢复后重新扫描，文档重新可用。
    fs::rename(&offline, dir.path()).unwrap();
    let recovered = scan(&mut conn, &source.id);
    assert!(recovered.available);
    let restored = kb_service::list_documents(&conn, &KbFilter::default()).unwrap();
    assert!(restored.iter().all(|doc| doc.available));
}

#[test]
fn remove_source_drops_documents_and_topics() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &source.id);
    assert!(kb_service::overview(&conn, 50).unwrap().doc_count > 0);

    assert!(kb_service::remove_source(&conn, &source.id).unwrap());
    let overview = kb_service::overview(&conn, 50).unwrap();
    assert_eq!(overview.doc_count, 0);
    assert_eq!(overview.topic_count, 0);
}

#[test]
fn search_uses_fts_then_like_fallback() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &source.id);

    let fts = kb_service::search(&conn, "注意力经济", Some(20)).unwrap();
    assert!(!fts.is_empty());
    assert_eq!(fts[0].matched_by, "fts");

    // 两字查询短于 FTS 下限，走 LIKE 回退。
    let like = kb_service::search(&conn, "文档", Some(20)).unwrap();
    assert!(like.len() >= 2);
    assert!(like.iter().all(|hit| hit.matched_by == "like"));

    assert!(kb_service::search(&conn, "   ", Some(20)).unwrap().is_empty());
}

#[test]
fn overview_reports_domains_topics_and_rings() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &source.id);

    let overview = kb_service::overview(&conn, 50).unwrap();
    assert_eq!(overview.source_count, 1);
    assert_eq!(overview.available_sources, 1);
    assert_eq!(overview.doc_count, 5);
    assert!(overview.topic_count >= 3);
    assert!(overview.domains.iter().any(|stat| stat.domain == "未归类" && stat.doc_count == 4));
    assert!(overview.domains.iter().any(|stat| stat.domain == "项目" && stat.doc_count == 1));
    let ring_total: i64 = overview.rings.iter().map(|ring| ring.added).sum();
    assert_eq!(ring_total, 5);
}

#[test]
fn document_filter_narrows_results() {
    let dir = setup();
    let mut conn = db();
    let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
    scan(&mut conn, &source.id);

    let filtered = kb_service::list_documents(
        &conn,
        &KbFilter {
            domain: Some("项目".to_string()),
            ..KbFilter::default()
        },
    )
    .unwrap();
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].normalized_name.contains("方案"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// 属性：重复扫描不新增文档，索引规模稳定。
    #[test]
    fn property_rescan_is_stable(extra in 0usize..4) {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..extra {
            write(dir.path(), &format!("记录{index}.md"), "内容");
        }
        let mut conn = db();
        let source = kb_service::add_source(&conn, &dir.path().to_string_lossy()).unwrap();
        let first = kb_service::scan_source(&mut conn, &source.id).unwrap();
        prop_assert_eq!(first.added as usize, extra);

        for _ in 0..3 {
            let again = kb_service::scan_source(&mut conn, &source.id).unwrap();
            prop_assert_eq!(again.added, 0);
            prop_assert_eq!(again.removed, 0);
        }
        prop_assert_eq!(kb_service::list_documents(&conn, &KbFilter::default()).unwrap().len(), extra);
    }

    /// 属性：来源不可用时，既有文档全部保留且标记为不可读。
    #[test]
    fn property_offline_source_retains_documents(count in 1usize..4) {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..count {
            write(dir.path(), &format!("笔记{index}.md"), "内容");
        }
        let mut conn = db();
        let path = dir.path().to_string_lossy().to_string();
        let source = kb_service::add_source(&conn, &path).unwrap();
        kb_service::scan_source(&mut conn, &source.id).unwrap();

        let offline = format!("{path}-moved");
        fs::rename(dir.path(), &offline).unwrap();
        let outcome = kb_service::scan_source(&mut conn, &source.id).unwrap();
        prop_assert!(!outcome.available);
        let docs = kb_service::list_documents(&conn, &KbFilter::default()).unwrap();
        prop_assert_eq!(docs.len(), count);
        prop_assert!(docs.iter().all(|doc| !doc.available));
    }
}
