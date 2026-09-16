//! 跨模块的小工具。

use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成带前缀的短标识。种子相同也保证同进程内不重复。
pub fn unique_id(prefix: &str, seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("{prefix}-{}", &digest[..16])
}

/// 对一段文本取 SHA-256 十六进制摘要，用于内容与元数据哈希。
pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}
