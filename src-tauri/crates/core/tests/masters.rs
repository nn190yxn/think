use std::path::Path;

use serde_json::{json, Value};
use thought_forge_core::corpus::repo as corpus;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::error::CoreError;
use thought_forge_core::master::{repo as masters, Layer};

fn memory_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

/// 写入一个大师包目录，返回目录句柄。
fn write_pack(manifest: &Value, corpus_body: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("临时目录");
    std::fs::write(
        dir.path().join("master.json"),
        serde_json::to_string_pretty(manifest).unwrap(),
    )
    .unwrap();
    let corpus_dir = dir.path().join("corpus");
    std::fs::create_dir_all(&corpus_dir).unwrap();
    std::fs::write(corpus_dir.join("notes.md"), corpus_body).unwrap();
    dir
}

fn valid_manifest(version: i64, units: Vec<Value>) -> Value {
    json!({
        "format": "thought-forge.master-pack",
        "formatVersion": 1,
        "id": "okada",
        "name": "稻盛和夫",
        "domain": "经营哲学",
        "layers": ["dao", "fa", "qi"],
        "version": version,
        "summary": "以心性为本的经营观",
        "style": "朴素、直接、反复强调动机",
        "blindSpots": "对高速变化的行业反应偏慢",
        "note": "首版",
        "units": units,
        "corpus": [
            { "ref": "corpus/notes.md", "kind": "book", "title": "活法 读书笔记", "locationHint": "全篇" }
        ]
    })
}

fn unit(title: &str, layer: &str) -> Value {
    json!({
        "title": title,
        "layer": layer,
        "triggerCondition": "当需要对一件事做长期取舍时",
        "steps": ["先问动机", "再看是否利他", "最后定取舍"],
        "mechanism": "把判断拉回到心性上",
        "boundary": "适用于长期经营，不适用于需要快速试错的早期探索",
        "evidence": [
            { "corpusRef": "corpus/notes.md", "excerpt": "动机至善，私心了无", "location": "第一章" }
        ]
    })
}

fn expect_pack_error(result: Result<Value, CoreError>, keyword: &str) {
    match result {
        Err(CoreError::PackInvalid(issues)) => {
            let joined = issues.join("；");
            assert!(
                joined.contains(keyword),
                "未包含「{keyword}」，实际为：{joined}"
            );
        }
        Err(other) => panic!("期望校验失败，实际为：{other}"),
        Ok(_) => panic!("期望校验失败，实际通过"),
    }
}

#[test]
fn rejects_unit_missing_four_elements() {
    let mut broken = unit("缺少边界", "dao");
    broken["boundary"] = json!("");
    let dir = write_pack(&valid_manifest(1, vec![broken]), "正文");
    let err = masters::validate_only(dir.path()).map(|_| json!(null));
    expect_pack_error(err, "缺少适用边界");
}

#[test]
fn rejects_invalid_layer_and_layer_outside_declaration() {
    let dir = write_pack(&valid_manifest(1, vec![unit("层次非法", "liu")]), "正文");
    expect_pack_error(masters::validate_only(dir.path()), "取值不合法");

    // 层次合法但没有在大师声明中列出，同样拒绝。
    let dir = write_pack(&valid_manifest(1, vec![unit("层次越界", "shu")]), "正文");
    expect_pack_error(masters::validate_only(dir.path()), "不在大师声明的 layers 内");
}

#[test]
fn rejects_duplicate_titles_and_unknown_corpus_ref() {
    let dir = write_pack(
        &valid_manifest(1, vec![unit("同名", "dao"), unit("同名", "fa")]),
        "正文",
    );
    expect_pack_error(masters::validate_only(dir.path()), "标题重复");

    let mut bad_ref = unit("来源非法", "dao");
    bad_ref["evidence"][0]["corpusRef"] = json!("corpus/missing.md");
    let dir = write_pack(&valid_manifest(1, vec![bad_ref]), "正文");
    expect_pack_error(masters::validate_only(dir.path()), "未声明的语料");
}

#[test]
fn rejects_missing_evidence_and_missing_corpus_file() {
    let mut no_evidence = unit("没有来源", "dao");
    no_evidence["evidence"] = json!([]);
    let dir = write_pack(&valid_manifest(1, vec![no_evidence]), "正文");
    expect_pack_error(masters::validate_only(dir.path()), "缺少来源标注");

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("master.json"),
        serde_json::to_string(&valid_manifest(1, vec![unit("语料缺失", "dao")])).unwrap(),
    )
    .unwrap();
    expect_pack_error(masters::validate_only(dir.path()), "语料文件不存在");
}

#[test]
fn installs_first_version_with_citations_and_searchable_corpus() {
    let dir = write_pack(
        &valid_manifest(1, vec![unit("心性优先", "dao"), unit("阿米巴经营", "fa")]),
        "动机至善，私心了无。",
    );
    let mut conn = memory_db();
    let outcome = masters::install(&mut conn, dir.path()).expect("安装成功");

    assert!(outcome.created);
    assert_eq!(outcome.version, 1);
    assert_eq!(outcome.unit_count, 2);
    assert_eq!(outcome.corpus_count, 1);
    assert_eq!(outcome.diff.added.len(), 2);
    assert_eq!(outcome.diff.carried, 0);

    let detail = masters::detail(&conn, "okada").expect("可读取详情");
    assert_eq!(detail.name, "稻盛和夫");
    assert_eq!(detail.layers, vec![Layer::Dao, Layer::Fa, Layer::Qi]);
    assert_eq!(detail.units.len(), 2);
    // 每个技能单元都带可溯源引用，且语料存在。
    for unit in &detail.units {
        assert_eq!(unit.citations.len(), 1);
        assert!(unit.citations[0].available);
        assert!(unit.citations[0].corpus_item_id.is_some());
    }

    let items = corpus::list(&conn, Some("okada")).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "活法 读书笔记");
    assert!(items[0].available);

    // 长查询走 FTS5，短查询走回退路径。
    let hits = corpus::search(&conn, "读书笔记", None).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].matched_by, "fts");
    let short = corpus::search(&conn, "活法", None).unwrap();
    assert_eq!(short.len(), 1);
    assert_eq!(short[0].matched_by, "like");
}

#[test]
fn layer_questions_cover_the_six_known_prompts() {
    let expected = [
        (Layer::Dao, "什么值得做"),
        (Layer::Fa, "规律是什么"),
        (Layer::Shu, "具体怎么做"),
        (Layer::Qi, "靠什么心力度过"),
        (Layer::Tool, "用什么载体放大"),
        (Layer::Shi, "现在是不是时候"),
    ];
    for (layer, question) in expected {
        assert_eq!(layer.question(), question, "{} 的核心问题应稳定", layer.name());
    }
}

#[test]
fn detail_reports_six_question_profile_with_gaps() {
    let dir = write_pack(
        &valid_manifest(1, vec![unit("心性优先", "dao"), unit("阿米巴经营", "fa")]),
        "动机至善，私心了无。",
    );
    let mut conn = memory_db();
    masters::install(&mut conn, dir.path()).expect("安装成功");
    let detail = masters::detail(&conn, "okada").expect("可读取详情");

    let profile = &detail.layer_profile;
    assert_eq!(profile.len(), 6, "六题各占一行");
    let order: Vec<Layer> = profile.iter().map(|entry| entry.layer).collect();
    assert_eq!(
        order,
        vec![
            Layer::Dao,
            Layer::Fa,
            Layer::Shu,
            Layer::Qi,
            Layer::Tool,
            Layer::Shi,
        ],
        "六题档案按道法术气器势排列"
    );
    for entry in profile {
        assert_eq!(entry.name, entry.layer.name());
        assert_eq!(entry.question, entry.layer.question());
        assert_eq!(entry.unit_count as usize, entry.unit_titles.len());
    }
    assert_eq!(profile[0].unit_titles, vec!["心性优先".to_string()]);
    assert_eq!(profile[1].unit_titles, vec!["阿米巴经营".to_string()]);
    // 空缺题保留且计数为零，空缺本身是有效信息。
    for entry in &profile[2..] {
        assert_eq!(entry.unit_count, 0);
        assert!(entry.unit_titles.is_empty());
    }
}

#[test]
fn second_version_carries_forward_and_keeps_snapshot() {
    let mut conn = memory_db();

    let first = write_pack(
        &valid_manifest(1, vec![unit("心性优先", "dao"), unit("阿米巴经营", "fa")]),
        "第一版语料",
    );
    masters::install(&mut conn, first.path()).expect("首版安装");

    // 第二版：保留两条旧单元，改写一条，新增一条。
    let mut revised = unit("心性优先", "dao");
    revised["boundary"] = json!("修订后的边界，收窄到经营决策");
    let second = write_pack(
        &valid_manifest(
            2,
            vec![revised, unit("阿米巴经营", "fa"), unit("六项精进", "qi")],
        ),
        "第二版语料",
    );
    let outcome = masters::install(&mut conn, second.path()).expect("增量更新");

    assert!(!outcome.created);
    assert_eq!(outcome.version, 2);
    assert_eq!(outcome.unit_count, 3);
    assert_eq!(outcome.diff.added, vec!["六项精进"]);
    assert_eq!(outcome.diff.updated, vec!["心性优先"]);
    assert_eq!(outcome.diff.carried, 1);

    let detail = masters::detail(&conn, "okada").unwrap();
    assert_eq!(detail.current_version, 2);
    assert_eq!(detail.units.len(), 3);
    let revised_unit = detail
        .units
        .iter()
        .find(|unit| unit.title == "心性优先")
        .unwrap();
    assert!(revised_unit.boundary.contains("修订后"));
    // 从上一版原样带入的单元，引用依旧可溯源。
    let carried = detail
        .units
        .iter()
        .find(|unit| unit.title == "阿米巴经营")
        .unwrap();
    assert_eq!(carried.citations.len(), 1);
    assert!(carried.citations[0].corpus_item_id.is_some());

    // 两个版本的快照都在。
    assert_eq!(detail.versions.len(), 2);
    assert_eq!(detail.versions[0].version, 2);
    assert_eq!(detail.versions[1].unit_count, 2);
}

#[test]
fn version_must_increase_and_revert_keeps_higher_snapshot() {
    let mut conn = memory_db();
    masters::install(
        &mut conn,
        write_pack(&valid_manifest(1, vec![unit("甲", "dao")]), "语料").path(),
    )
    .unwrap();

    // 同版本重装被拒绝。
    let same = write_pack(&valid_manifest(1, vec![unit("甲", "dao")]), "语料");
    let err = masters::install(&mut conn, same.path()).unwrap_err();
    assert!(matches!(err, CoreError::InvalidInput(_)));

    // 首版版本号不为 1 也被拒绝。
    let mut skipped = valid_manifest(3, vec![unit("甲", "dao")]);
    skipped["id"] = json!("munger");
    let dir = write_pack(&skipped, "语料");
    assert!(matches!(
        masters::install(&mut conn, dir.path()).unwrap_err(),
        CoreError::InvalidInput(_)
    ));

    masters::install(
        &mut conn,
        write_pack(&valid_manifest(2, vec![unit("乙", "fa")]), "语料").path(),
    )
    .unwrap();
    assert_eq!(masters::detail(&conn, "okada").unwrap().current_version, 2);

    // 回退到 v1：当前版本指针回到 1，v2 快照仍在。
    assert_eq!(masters::revert(&conn, "okada", 1).unwrap(), 1);
    let detail = masters::detail(&conn, "okada").unwrap();
    assert_eq!(detail.current_version, 1);
    // v1 只有一条单元，v2 的快照不影响回退后的读数。
    assert_eq!(detail.units.len(), 1);
    assert_eq!(detail.versions.len(), 2);

    // 回退到不存在的版本报未找到。
    assert!(matches!(
        masters::revert(&conn, "okada", 9).unwrap_err(),
        CoreError::NotFound(_)
    ));
}

#[test]
fn coverage_matrix_flags_missing_layers() {
    let mut conn = memory_db();
    masters::install(
        &mut conn,
        write_pack(&valid_manifest(1, vec![unit("甲", "dao")]), "语料").path(),
    )
    .unwrap();

    let matrix = masters::coverage_matrix(&conn).unwrap();
    assert_eq!(matrix.master_count, 1);
    assert_eq!(matrix.layers.len(), 6);

    let dao = matrix
        .layers
        .iter()
        .find(|entry| entry.layer == Layer::Dao)
        .unwrap();
    assert_eq!(dao.master_count, 1);
    assert_eq!(dao.unit_count, 1);
    assert_eq!(dao.masters, vec!["稻盛和夫".to_string()]);

    let shi = matrix
        .layers
        .iter()
        .find(|entry| entry.layer == Layer::Shi)
        .unwrap();
    assert_eq!(shi.master_count, 0);

    // 势、术、器三层空缺，给出三条补充建议。
    assert_eq!(matrix.suggestions.len(), 3);
    assert!(matrix
        .suggestions
        .iter()
        .any(|text| text.contains("势层尚无大师")));

    let domain = matrix
        .domains
        .iter()
        .find(|entry| entry.domain == "经营哲学")
        .unwrap();
    assert_eq!(domain.master_count, 1);
    assert_eq!(domain.present_layers, vec![Layer::Dao, Layer::Fa, Layer::Qi]);
    assert!(domain.missing_layers.contains(&Layer::Shi));
}

#[test]
fn corpus_removal_marks_source_missing_but_keeps_master() {
    let dir = write_pack(&valid_manifest(1, vec![unit("心性优先", "dao")]), "语料正文");
    let mut conn = memory_db();
    masters::install(&mut conn, dir.path()).unwrap();

    // 删除语料文件，模拟来源失效。
    std::fs::remove_file(dir.path().join("corpus/notes.md")).unwrap();

    let detail = masters::detail(&conn, "okada").unwrap();
    assert_eq!(detail.units.len(), 1);
    let citation = &detail.units[0].citations[0];
    assert!(!citation.available);
    // 摘录仍保留，大师包照常可用。
    assert!(citation.excerpt.contains("动机至善"));

    let items = corpus::list(&conn, None).unwrap();
    assert_eq!(items.len(), 1);
    assert!(!items[0].available);
    assert_eq!(corpus::unavailable_count(&conn).unwrap(), 1);
}

#[test]
fn flags_unit_and_lists_domains() {
    let mut conn = memory_db();
    masters::install(
        &mut conn,
        write_pack(&valid_manifest(1, vec![unit("心性优先", "dao")]), "语料").path(),
    )
    .unwrap();

    let detail = masters::detail(&conn, "okada").unwrap();
    let unit_id = detail.units[0].id.clone();

    masters::flag_unit(&conn, &unit_id, "在快速试错场景不适用").unwrap();
    let flagged = masters::detail(&conn, "okada").unwrap();
    assert_eq!(
        flagged.units[0].flagged_reason.as_deref(),
        Some("在快速试错场景不适用")
    );

    assert!(matches!(
        masters::flag_unit(&conn, "not-a-unit", "理由").unwrap_err(),
        CoreError::NotFound(_)
    ));
    assert!(matches!(
        masters::flag_unit(&conn, &unit_id, "   ").unwrap_err(),
        CoreError::InvalidInput(_)
    ));

    assert_eq!(masters::domains(&conn).unwrap(), vec!["经营哲学"]);
    assert!(masters::exists(&conn, "okada").unwrap());
    assert!(!masters::exists(&conn, "nobody").unwrap());
}

#[test]
fn list_filters_by_domain_and_layer() {
    let mut conn = memory_db();
    masters::install(
        &mut conn,
        write_pack(&valid_manifest(1, vec![unit("甲", "dao")]), "语料").path(),
    )
    .unwrap();

    assert_eq!(masters::list(&conn, None, None).unwrap().len(), 1);
    assert_eq!(masters::list(&conn, Some("经营哲学"), None).unwrap().len(), 1);
    assert_eq!(masters::list(&conn, Some("哲学"), None).unwrap().len(), 0);
    assert_eq!(masters::list(&conn, None, Some(Layer::Dao)).unwrap().len(), 1);
    assert_eq!(masters::list(&conn, None, Some(Layer::Shi)).unwrap().len(), 0);

    let summary = &masters::list(&conn, None, None).unwrap()[0];
    assert_eq!(summary.unit_count, 1);
    assert_eq!(summary.current_version, 1);
}

#[test]
fn external_absolute_corpus_path_is_supported() {
    let external = tempfile::tempdir().unwrap();
    let book = external.path().join("book.txt");
    std::fs::write(&book, "外部语料正文").unwrap();

    let mut manifest = valid_manifest(1, vec![unit("外部来源", "dao")]);
    let absolute = book.to_string_lossy().to_string();
    manifest["corpus"][0]["ref"] = json!(absolute);
    manifest["units"][0]["evidence"][0]["corpusRef"] = json!(absolute);
    let dir = write_pack(&manifest, "未被使用的包内语料");

    let mut conn = memory_db();
    let outcome = masters::install(&mut conn, dir.path()).expect("外部语料可安装");
    assert_eq!(outcome.corpus_count, 1);

    let items = corpus::list(&conn, None).unwrap();
    assert!(items[0].available);
    assert!(Path::new(&items[0].source_ref).is_absolute());
}
