//! 随应用分发的种子大师包必须能直接安装，且六层各取一位覆盖完整。

use std::path::PathBuf;

use thought_forge_core::corpus;
use thought_forge_core::db::{self, migrations};
use thought_forge_core::master::{repo, Layer, LAYER_ORDER};
use thought_forge_core::council::pool::{self as pool_repo, TopicInput};

fn memory_db() -> rusqlite::Connection {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");
    conn
}

fn seed_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../seed-packs")
}

fn pack_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(seed_root())
        .expect("种子目录应存在")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.join("master.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

#[test]
fn every_seed_pack_installs_and_covers_all_six_layers() {
    let dirs = pack_dirs();
    assert_eq!(dirs.len(), 6, "六层各一位大师，共六个种子包");

    let mut conn = memory_db();

    for dir in &dirs {
        let outcome = repo::install(&mut conn, dir)
            .unwrap_or_else(|error| panic!("{} 安装失败：{error}", dir.display()));
        assert!(outcome.created, "首装应标记为新建");
        assert_eq!(outcome.version, 1);
        assert!(outcome.unit_count > 0);
        assert!(outcome.corpus_count > 0);
    }

    let matrix = repo::coverage_matrix(&conn).unwrap();
    assert_eq!(matrix.master_count, 6);
    assert!(matrix.suggestions.is_empty(), "六层应无空缺");

    for entry in &matrix.layers {
        assert!(
            entry.master_count > 0,
            "{}层缺少大师",
            entry.name
        );
        assert!(entry.unit_count > 0, "{}层缺少技能单元", entry.name);
    }

    // 覆盖矩阵的层次顺序固定为道法术气器势。
    let order: Vec<Layer> = matrix.layers.iter().map(|entry| entry.layer).collect();
    assert_eq!(order, LAYER_ORDER.to_vec());
}

#[test]
fn every_seed_pack_covers_all_six_layers_in_its_own_units() {
    let mut conn = memory_db();
    for dir in &pack_dirs() {
        repo::install(&mut conn, dir).unwrap();
    }

    let topic = TopicInput {
        question: "该不该动手",
        domains: &[],
    };
    let masters = pool_repo::build(&conn, &topic).unwrap();
    assert_eq!(masters.candidates.len(), 6, "六个种子包各一位大师");

    for master in &masters.candidates {
        for layer in LAYER_ORDER {
            let depth = master.layer_depth.get(&layer).copied().unwrap_or(0);
            assert!(
                depth > 0,
                "{} 在 {:?} 题没有单元，六题收口未达成",
                master.name,
                layer
            );
        }
    }
}

#[test]
fn every_seed_pack_corpus_is_searchable_by_its_title() {
    let mut conn = memory_db();
    for dir in &pack_dirs() {
        repo::install(&mut conn, dir).unwrap();
    }

    let items = corpus::repo::list(&conn, None).unwrap();
    assert_eq!(items.len(), 6, "六个种子包各一份语料");

    for item in &items {
        let hits = corpus::repo::search(&conn, &item.title, None).unwrap();
        for master_id in &item.master_ids {
            assert!(
                hits.iter().any(|hit| hit.item.master_ids.contains(master_id)),
                "语料《{}》按标题检索不到它的大师 {}",
                item.title,
                master_id
            );
        }
    }
}

#[test]
fn seed_corpus_is_searchable_after_install() {
    let mut conn = memory_db();
    for dir in pack_dirs() {
        repo::install(&mut conn, &dir).unwrap();
    }

    let hits = corpus::repo::search(&conn, "杠杆", None).unwrap();
    assert!(
        hits.iter()
            .any(|hit| hit.item.master_ids.iter().any(|id| id == "naval-ravikant")),
        "检索应命中纳瓦尔的语料"
    );

    let items = corpus::repo::list(&conn, None).unwrap();
    assert_eq!(items.len(), 6);
    assert!(items.iter().all(|item| item.available));
}
