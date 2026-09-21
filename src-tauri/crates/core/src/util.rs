//! 跨模块的小工具。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成带前缀的短标识。种子相同也保证不重复：进程内靠计数器，跨进程靠时间片。
pub fn unique_id(prefix: &str, seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    // 计数器每次启动都从 0 开始：只靠它会跟历史记录撞号（同一个问题在第二次启动时
    // 算出同一个 id，插入时报 UNIQUE 冲突），所以再混入时间片。
    hasher.update(nonce().to_le_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("{prefix}-{}", &digest[..16])
}

/// 时间片：纳秒级，用来区分先后两次调用。
fn nonce() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|delta| delta.as_nanos() as u64)
        .unwrap_or(0)
}

/// 对一段文本取 SHA-256 十六进制摘要，用于内容与元数据哈希。
pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_stay_unique_for_the_same_seed() {
        let first = unique_id("council", "同一个问题");
        let second = unique_id("council", "同一个问题");
        assert_ne!(first, second, "同种子连续两次也必须不同");
        assert!(first.starts_with("council-"), "前缀应保留");
        assert_eq!(first.len(), "council-".len() + 16, "长度约定不变");
    }

    #[test]
    fn nonce_advances_between_calls() {
        // 跟历史记录撞号的根因是计数器每次启动归零；时间片必须自己往前走。
        let first = nonce();
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(nonce() > first, "时间片应随调用前进");
    }
}
