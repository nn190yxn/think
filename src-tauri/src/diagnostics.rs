//! 诊断包导出：把界面手上的体检与配置摘要写成一份 JSON 文件。
//!
//! 只接收文件名与正文：文件名受约束，正文原样落盘不做解析。密钥本体不参与拼装，
//! 拼装侧只写「有没有写入」这类布尔结论，落盘侧再兜一道文件名与体积的闸。

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::protocol::CommandResult;
use crate::state::AppState;

/// 正文上限：诊断包是摘要，不该塞进大文件。
const MAX_BYTES: usize = 1024 * 1024;

/// 落盘结果：路径与字节数。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsFileDto {
    pub path: String,
    pub bytes: usize,
}

/// 拼出落盘路径：只允许 `diagnostics-*.json` 这种名字，且只落在数据目录里。
pub fn target_path(db_dir: &Path, file_name: &str) -> Result<PathBuf, String> {
    let shape_ok = file_name.starts_with("diagnostics-") && file_name.ends_with(".json");
    let chars_ok = file_name.len() <= 64
        && file_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_'));
    if !shape_ok || !chars_ok {
        return Err("诊断包文件名不符合约定".to_string());
    }
    Ok(db_dir.join(file_name))
}

/// 写出诊断包，返回路径与字节数。
pub fn write(db_dir: &Path, file_name: &str, content: &str) -> Result<(String, usize), String> {
    if content.len() > MAX_BYTES {
        return Err("诊断包正文过大".to_string());
    }
    let path = target_path(db_dir, file_name)?;
    std::fs::write(&path, content).map_err(|error| format!("诊断包写入失败：{error}"))?;
    Ok((path.to_string_lossy().to_string(), content.len()))
}

/// 把一份诊断摘要写进数据目录，供排查问题时转交。
#[tauri::command]
pub fn diagnostics_write(
    state: State<'_, AppState>,
    file_name: String,
    content: String,
) -> CommandResult<DiagnosticsFileDto> {
    let Some(dir) = state.db_path.parent() else {
        return CommandResult::failure("E_IO", "数据目录不可用");
    };
    match write(dir, &file_name, &content) {
        Ok((path, bytes)) => CommandResult::ok(DiagnosticsFileDto { path, bytes }),
        Err(message) => CommandResult::failure("E_INVALID_INPUT", message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("forge-diagnostics-{}", std::process::id()))
    }

    #[test]
    fn rejects_names_outside_the_convention() {
        let dir = temp_dir();
        for name in ["a.json", "diagnostics-1.txt", "diagnostics-1.json.bak", ""] {
            assert!(target_path(&dir, name).is_err(), "{name} 不应通过");
        }
    }

    #[test]
    fn rejects_traversal_and_non_ascii_characters() {
        let dir = temp_dir();
        for name in [
            "diagnostics-../evil.json",
            "diagnostics-a/b.json",
            "diagnostics-中文.json",
        ] {
            assert!(target_path(&dir, name).is_err(), "{name} 不应通过");
        }
    }

    #[test]
    fn rejects_oversized_payload() {
        let dir = temp_dir();
        let big = "x".repeat(MAX_BYTES + 1);
        assert!(write(&dir, "diagnostics-big.json", &big).is_err());
    }

    #[test]
    fn writes_only_inside_the_data_directory() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let (path, bytes) = write(&dir, "diagnostics-2026-09-21.json", "{\"a\":1}").expect("写出一份");
        assert!(
            PathBuf::from(&path).starts_with(&dir),
            "诊断包只能落在数据目录里"
        );
        assert_eq!(bytes, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
