use rusqlite::Connection;

use crate::error::{CoreError, CoreResult};

const MAX_KEY_LEN: usize = 64;
const MAX_VALUE_LEN: usize = 4096;

fn validate(key: &str, value_len: usize) -> CoreResult<()> {
    if key.trim().is_empty() {
        return Err(CoreError::InvalidInput("设置项的键不能为空".into()));
    }
    if key.len() > MAX_KEY_LEN {
        return Err(CoreError::InvalidInput(format!(
            "设置项的键过长：上限 {MAX_KEY_LEN} 字节"
        )));
    }
    if value_len > MAX_VALUE_LEN {
        return Err(CoreError::InvalidInput(format!(
            "设置项的值过长：上限 {MAX_VALUE_LEN} 字节"
        )));
    }
    Ok(())
}

pub fn get(conn: &Connection, key: &str) -> CoreResult<Option<String>> {
    validate(key, 0)?;
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(rusqlite::params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

pub fn set(conn: &Connection, key: &str, value: &str) -> CoreResult<()> {
    validate(key, value.len())?;
    conn.execute(
        "INSERT INTO settings (key, value, updated_at)
         VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
         ON CONFLICT(key) DO UPDATE SET
             value = excluded.value,
             updated_at = excluded.updated_at",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
