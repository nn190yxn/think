//! 大师对立度的离线预计算。
//!
//! 对立度随大师包安装与更新重算并存表，会诊时直接读取，避免每次做全量比对。

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::error::CoreResult;
use crate::master::Layer;

use super::pool::load_master_texts;
use super::scoring;

fn key(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

/// 读取全部已计算的对立度，键为按 id 归一化排序的大师对。
pub fn load(conn: &Connection) -> CoreResult<BTreeMap<(String, String), f64>> {
    let mut stmt = conn.prepare(
        "SELECT master_a_id, master_b_id, opposition_score FROM master_pairings",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (a, b, score) = row?;
        map.insert((a, b), score);
    }
    Ok(map)
}

/// 查询两位大师之间的对立度，未计算时返回 0。
pub fn mutual(map: &BTreeMap<(String, String), f64>, left: &str, right: &str) -> f64 {
    map.get(&key(left, right)).copied().unwrap_or(0.0)
}

/// 读取按题预计算的对立度，键为归一化的大师对，值为「题 → 对立度」。
pub fn load_by_layer(
    conn: &Connection,
) -> CoreResult<BTreeMap<(String, String), BTreeMap<Layer, f64>>> {
    let mut stmt = conn.prepare(
        "SELECT master_a_id, master_b_id, layer, opposition_score FROM master_layer_pairings",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, f64>(3)?,
        ))
    })?;
    let mut map: BTreeMap<(String, String), BTreeMap<Layer, f64>> = BTreeMap::new();
    for row in rows {
        let (a, b, layer_name, score) = row?;
        let Some(layer) = Layer::parse(&layer_name) else {
            continue;
        };
        map.entry((a, b)).or_default().insert(layer, score);
    }
    Ok(map)
}

/// 查询两位大师在某一题上的对立度，未计算时返回 `None`。
pub fn mutual_layer(
    map: &BTreeMap<(String, String), BTreeMap<Layer, f64>>,
    left: &str,
    right: &str,
    layer: Layer,
) -> Option<f64> {
    map.get(&key(left, right))
        .and_then(|by_layer| by_layer.get(&layer))
        .copied()
}

/// 重算全部大师对的对立度，返回写入的对数。
pub fn recompute(conn: &Connection) -> CoreResult<usize> {
    let masters = load_master_texts(conn)?;
    let computed_at: String =
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })?;

    let mut written = 0usize;
    for (index, left) in masters.iter().enumerate() {
        for right in masters.iter().skip(index + 1) {
            let (a, b) = key(&left.id, &right.id);
            let score = scoring::opposition(&left.tokens, &left.layers, &right.tokens, &right.layers);
            conn.execute(
                "INSERT INTO master_pairings (master_a_id, master_b_id, opposition_score, computed_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (master_a_id, master_b_id)
                 DO UPDATE SET opposition_score = excluded.opposition_score,
                               computed_at = excluded.computed_at",
                rusqlite::params![a, b, score, computed_at],
            )?;

            // 同题对立度逐层写：某一方在该题没有单元时仍记录一个值，
            // 由 `layer_opposition` 的退路口径决定，读取方无需再判断。
            for layer in crate::master::LAYER_ORDER {
                let empty = std::collections::BTreeSet::new();
                let a_tokens = left.layer_tokens.get(&layer).unwrap_or(&empty);
                let b_tokens = right.layer_tokens.get(&layer).unwrap_or(&empty);
                let layer_score =
                    scoring::layer_opposition(a_tokens, &left.layers, b_tokens, &right.layers);
                conn.execute(
                    "INSERT INTO master_layer_pairings
                         (master_a_id, master_b_id, layer, opposition_score, computed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT (master_a_id, master_b_id, layer)
                     DO UPDATE SET opposition_score = excluded.opposition_score,
                                   computed_at = excluded.computed_at",
                    rusqlite::params![a, b, layer.as_str(), layer_score, computed_at],
                )?;
            }
            written += 1;
        }
    }

    // 清理已不再成对存在的历史记录。
    conn.execute(
        "DELETE FROM master_pairings
         WHERE master_a_id NOT IN (SELECT id FROM masters)
            OR master_b_id NOT IN (SELECT id FROM masters)",
        [],
    )?;
    conn.execute(
        "DELETE FROM master_layer_pairings
         WHERE master_a_id NOT IN (SELECT id FROM masters)
            OR master_b_id NOT IN (SELECT id FROM masters)",
        [],
    )?;

    Ok(written)
}
