//! 数据导出的归档生成、范围统计与确认后清除。

use std::path::Path;

use rusqlite::types::ValueRef;
use rusqlite::Connection;

use crate::db;
use crate::error::{CoreError, CoreResult};
use crate::util::unique_id;

use super::{
    DataEventView, DataScope, DataTableCount, ExportOutcome, PurgeOutcome, ARCHIVE_FORMAT,
    ARCHIVE_FORMAT_VERSION, DATA_TABLES, SCOPE_ALL,
};

const MAX_LIMIT: i64 = 200;

pub fn now(conn: &Connection) -> CoreResult<String> {
    let value: String = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [],
        |row| row.get(0),
    )?;
    Ok(value)
}

/// 统计归档范围内每张表的行数。
pub fn scope(conn: &Connection) -> CoreResult<DataScope> {
    let mut tables = Vec::new();
    let mut row_count = 0i64;
    for (table, label) in DATA_TABLES {
        let rows = db::count_rows(conn, table)?;
        row_count += rows;
        tables.push(DataTableCount {
            table: (*table).to_string(),
            label: (*label).to_string(),
            rows,
        });
    }
    Ok(DataScope {
        table_count: tables.len() as i64,
        tables,
        row_count,
    })
}

/// 把全部本地数据写成一个 JSON 归档，并在审计中留痕。
pub fn export(conn: &Connection, dest: &Path) -> CoreResult<ExportOutcome> {
    let mut archive = serde_json::Map::new();
    let mut table_count = 0i64;
    let mut row_count = 0i64;
    for (table, _label) in DATA_TABLES {
        if !db::table_exists(conn, table)? {
            continue;
        }
        let rows = dump_table(conn, table)?;
        row_count += rows.len() as i64;
        table_count += 1;
        archive.insert((*table).to_string(), serde_json::Value::Array(rows));
    }

    let created_at = now(conn)?;
    let document = serde_json::json!({
        "format": ARCHIVE_FORMAT,
        "formatVersion": ARCHIVE_FORMAT_VERSION,
        "exportedAt": created_at,
        "tables": serde_json::Value::Object(archive),
    });
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| CoreError::InvalidInput(format!("归档序列化失败：{error}")))?;
    std::fs::write(dest, &bytes)?;

    let location = dest.to_string_lossy().to_string();
    record_event(conn, "export", SCOPE_ALL, table_count, row_count, &location)?;

    Ok(ExportOutcome {
        path: location,
        table_count,
        row_count,
        bytes: bytes.len() as u64,
        created_at,
    })
}

/// 清除归档范围内的全部本地数据。清除范围与导出一致，先子表后父表。
pub fn purge(conn: &mut Connection, scope: &str) -> CoreResult<PurgeOutcome> {
    if scope != SCOPE_ALL {
        return Err(CoreError::InvalidInput(format!(
            "暂不支持的数据范围：{scope}"
        )));
    }
    let tx = conn.transaction()?;
    let mut table_count = 0i64;
    let mut row_count = 0i64;
    for (table, _label) in DATA_TABLES {
        if !db::table_exists(&tx, table)? {
            continue;
        }
        let sql = format!("DELETE FROM {table}");
        let removed = tx.execute(&sql, [])? as i64;
        row_count += removed;
        table_count += 1;
    }
    let created_at = now(&tx)?;
    tx.execute(
        "INSERT INTO data_events (id, kind, scope, table_count, row_count, location, created_at)
         VALUES (?1, 'purge', ?2, ?3, ?4, '', ?5)",
        rusqlite::params![
            unique_id("data", &created_at),
            scope,
            table_count,
            row_count,
            created_at
        ],
    )?;
    tx.commit()?;

    Ok(PurgeOutcome {
        scope: scope.to_string(),
        table_count,
        row_count,
        created_at,
    })
}

pub fn events(conn: &Connection, limit: i64) -> CoreResult<Vec<DataEventView>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let mut stmt = conn.prepare(
        "SELECT id, kind, scope, table_count, row_count, location, created_at
           FROM data_events ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        let kind: String = row.get(1)?;
        Ok(DataEventView {
            id: row.get(0)?,
            kind_label: if kind == "export" { "导出" } else { "清除" }.to_string(),
            kind,
            scope: row.get(2)?,
            table_count: row.get(3)?,
            row_count: row.get(4)?,
            location: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

fn record_event(
    conn: &Connection,
    kind: &str,
    scope: &str,
    table_count: i64,
    row_count: i64,
    location: &str,
) -> CoreResult<()> {
    let created_at = now(conn)?;
    conn.execute(
        "INSERT INTO data_events (id, kind, scope, table_count, row_count, location, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            unique_id("data", &format!("{kind}-{created_at}")),
            kind,
            scope,
            table_count,
            row_count,
            location,
            created_at
        ],
    )?;
    Ok(())
}

/// 读取整张表并转成 JSON 行数组。只用于导出，不参与业务读写。
fn dump_table(conn: &Connection, table: &str) -> CoreResult<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table}"))?;
    let column_names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut object = serde_json::Map::new();
        for (index, name) in column_names.iter().enumerate() {
            object.insert(name.clone(), value_to_json(row.get_ref(index)?));
        }
        out.push(serde_json::Value::Object(object));
    }
    Ok(out)
}

fn value_to_json(value: ValueRef<'_>) -> serde_json::Value {
    match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(number) => serde_json::Value::from(number),
        ValueRef::Real(number) => serde_json::Value::from(number),
        ValueRef::Text(bytes) => {
            serde_json::Value::from(String::from_utf8_lossy(bytes).to_string())
        }
        // 采集只存引用不存二进制正文，出现 blob 时以占位描述代替。
        ValueRef::Blob(bytes) => {
            serde_json::Value::from(format!("<blob:{} bytes>", bytes.len()))
        }
    }
}
