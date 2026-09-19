//! P4 性能门禁：十万节点下的图谱检索与过滤。

use std::time::Instant;

use thought_forge_core::db::{self, migrations};
use thought_forge_core::network::repo::{self, MAX_GRAPH_LIMIT};
use thought_forge_core::network::GraphFilter;

const NODES: usize = 100_000;

// 十万节点建库是分钟级开销，不进默认门禁：每次 `cargo test` 都重跑它会把内核
// 门禁拖长一个量级。要跑用 `pnpm gate:perf`，两个云端工作流各有独立步骤覆盖。
#[test]
#[ignore = "十万节点建库是分钟级开销，用 `pnpm gate:perf` 单独跑"]
fn graph_retrieval_scales_to_one_hundred_thousand_nodes() {
    let mut conn = db::open_in_memory().expect("内存库可打开");
    migrations::apply_all(&mut conn).expect("迁移可执行");

    let tx = conn.transaction().expect("事务可开启");
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO thought_nodes
                    (id, kind, content, normalized_content, activation,
                     activation_updated_at, created_at)
                 VALUES (?1, 'idea', ?2, ?2, ?3, '2026-09-14T00:00:00Z', '2026-09-14T00:00:00Z')",
            )
            .expect("语句可准备");
        for index in 0..NODES {
            let content = format!("节点-{index:06}");
            stmt.execute(rusqlite::params![
                format!("n{index:06}"),
                content,
                (index % 100) as f64 / 100.0
            ])
            .expect("节点可写入");
        }
    }
    tx.commit().expect("事务可提交");

    let start = Instant::now();
    let view = repo::get_graph(
        &conn,
        &GraphFilter {
            limit: Some(MAX_GRAPH_LIMIT),
            ..GraphFilter::default()
        },
    )
    .expect("图谱可读取");
    let elapsed = start.elapsed();

    assert_eq!(view.nodes.len(), MAX_GRAPH_LIMIT as usize);
    assert!(view.truncated, "超出上限的节点应被标记为截断");
    assert!(
        elapsed.as_millis() < 3000,
        "十万节点图谱检索耗时 {elapsed:?}，超出预算"
    );
}
