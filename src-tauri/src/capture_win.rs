//! Windows 原生采集探针。
//!
//! 只负责取原始数据并返回普通字符串，不依赖内核类型，因此可以在不带完整
//! Windows 工具链的机器上用 `cargo check --target x86_64-pc-windows-msvc` 单独校验。
//! 剪贴板可能被其他进程占用、目标进程可能拒绝查询，失败一律返回 `None`，
//! 由采集源跳过本轮并计入错误计数，不把失败升级成错误码。

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};

/// `CF_UNICODETEXT`，`windows_sys` 未导出这个剪贴板格式常量。
const CF_UNICODETEXT: u32 = 13;
/// `CF_DIB`，设备无关位图格式，用来判断剪贴板里是否有图片。
const CF_DIB: u32 = 8;
/// 进程映像路径的缓冲长度，足够容纳长路径。
const PATH_UNITS: usize = 1024;

/// UTF-16 缓冲转字符串，截到第一个 NUL。
fn wide_to_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    OsString::from_wide(&buffer[..end])
        .to_string_lossy()
        .into_owned()
}

/// 读取剪贴板文本。被占用、没有文本或内容为空时返回 `None`。
pub fn clipboard_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return None;
        }
        // 所有者传空表示由当前任务打开剪贴板；被其他进程占用时返回 0。
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let text = read_clipboard_text();
        CloseClipboard();
        text.filter(|value| !value.trim().is_empty())
    }
}

unsafe fn read_clipboard_text() -> Option<String> {
    let handle = GetClipboardData(CF_UNICODETEXT);
    if handle.is_null() {
        return None;
    }
    let pointer = GlobalLock(handle) as *const u16;
    if pointer.is_null() {
        return None;
    }
    // GlobalSize 给的是字节数，UTF-16 每个码元两字节。
    let units = (GlobalSize(handle) / 2) as usize;
    let text = if units == 0 {
        String::new()
    } else {
        wide_to_string(std::slice::from_raw_parts(pointer, units))
    };
    GlobalUnlock(handle);
    Some(text)
}

/// 剪贴板图片引用：只记格式与字节数，不读图片本体。
pub fn clipboard_image_ref() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_DIB) == 0 {
            return None;
        }
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_DIB);
        let size = if handle.is_null() {
            0
        } else {
            GlobalSize(handle)
        };
        CloseClipboard();
        Some(format!("clipboard:dib:{size}"))
    }
}

/// 前台窗口：返回（应用可执行文件名，窗口标题）。窗口标题取不到时为空串。
pub fn foreground_window() -> Option<(String, String)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let title = window_title(hwnd);
        let app = process_name(hwnd).unwrap_or_default();
        // 应用名与标题都拿不到时视为本轮无有效样本。
        if app.is_empty() && title.is_empty() {
            return None;
        }
        Some((app, title))
    }
}

unsafe fn window_title(hwnd: HWND) -> String {
    let length = GetWindowTextLengthW(hwnd);
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let written = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    if written <= 0 {
        return String::new();
    }
    wide_to_string(&buffer[..written as usize])
}

unsafe fn process_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 {
        return None;
    }
    // 只申请查询权限，避免为了取名而拿到不必要的进程控制权。
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if handle.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; PATH_UNITS];
    let mut size = buffer.len() as u32;
    let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
    CloseHandle(handle);
    if ok == 0 {
        return None;
    }
    let full = wide_to_string(&buffer[..size as usize]);
    if full.is_empty() {
        return None;
    }
    // 只保留可执行文件名，路径对采集没有意义。
    Some(
        full.rsplit(['\\', '/'])
            .next()
            .unwrap_or(full.as_str())
            .to_string(),
    )
}
