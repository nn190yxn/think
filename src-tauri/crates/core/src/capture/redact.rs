//! 脱敏规则引擎。
//!
//! 规则由内置模式与用户自定义词条两部分组成：内置模式覆盖邮箱、手机号、
//! 证件/卡号与长令牌这类常见敏感串；自定义词条用于遮住用户自己指定
//! 的公司名、项目代号等。命中一律以掩码替换。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 默认掩码。
pub const DEFAULT_MASK: &str = "[已脱敏]";

/// 长令牌判定的最小长度。
const TOKEN_MIN_LEN: usize = 24;
/// 数字串判定的长度区间，覆盖手机号、证件号与卡号。
const DIGIT_RUN_MIN: usize = 11;
const DIGIT_RUN_MAX: usize = 19;

/// 脱敏配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionRules {
    pub enabled: bool,
    #[serde(default)]
    pub terms: Vec<String>,
    #[serde(default = "default_mask")]
    pub mask: String,
}

fn default_mask() -> String {
    DEFAULT_MASK.to_string()
}

impl Default for RedactionRules {
    fn default() -> Self {
        Self {
            enabled: true,
            terms: Vec::new(),
            mask: default_mask(),
        }
    }
}

/// 单段文本的脱敏结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionOutcome {
    pub text: String,
    pub redacted: bool,
    pub hits: usize,
}

/// 对单段文本应用规则。
pub fn redact(text: &str, rules: &RedactionRules) -> RedactionOutcome {
    if !rules.enabled || text.is_empty() {
        return RedactionOutcome {
            text: text.to_string(),
            redacted: false,
            hits: 0,
        };
    }

    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<(usize, usize)> = Vec::new();

    // 自定义词条优先，按字符位置逐一匹配。
    for term in &rules.terms {
        if term.is_empty() {
            continue;
        }
        let term_chars: Vec<char> = term.chars().collect();
        let mut index = 0usize;
        while index + term_chars.len() <= chars.len() {
            if chars[index..index + term_chars.len()] == term_chars[..] {
                spans.push((index, index + term_chars.len()));
                index += term_chars.len();
            } else {
                index += 1;
            }
        }
    }

    // 内置模式：数字串、长令牌与邮箱。
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_ascii_digit() {
            let mut end = index;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            let len = end - index;
            if (DIGIT_RUN_MIN..=DIGIT_RUN_MAX).contains(&len) {
                spans.push((index, end));
            }
            index = end;
            continue;
        }
        if is_token_char(ch) {
            let mut end = index;
            while end < chars.len() && is_token_char(chars[end]) {
                end += 1;
            }
            if end - index >= TOKEN_MIN_LEN {
                spans.push((index, end));
            }
            index = end;
            continue;
        }
        if ch == '@' {
            if let Some(span) = email_span(&chars, index) {
                spans.push(span);
                index = span.1;
                continue;
            }
        }
        index += 1;
    }

    if spans.is_empty() {
        return RedactionOutcome {
            text: text.to_string(),
            redacted: false,
            hits: 0,
        };
    }

    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => {
                if end > last.1 {
                    last.1 = end;
                }
            }
            _ => merged.push((start, end)),
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for (start, end) in &merged {
        out.extend(&chars[cursor..*start]);
        out.push_str(&rules.mask);
        cursor = *end;
    }
    out.extend(&chars[cursor..]);

    RedactionOutcome {
        text: out,
        redacted: true,
        hits: merged.len(),
    }
}

/// 递归脱敏 JSON 中的所有字符串字段。
pub fn redact_value(value: &Value, rules: &RedactionRules) -> (Value, bool, usize) {
    match value {
        Value::String(text) => {
            let outcome = redact(text, rules);
            (
                Value::String(outcome.text),
                outcome.redacted,
                outcome.hits,
            )
        }
        Value::Array(items) => {
            let mut redacted = false;
            let mut hits = 0;
            let mapped: Vec<Value> = items
                .iter()
                .map(|item| {
                    let (next, did, count) = redact_value(item, rules);
                    redacted |= did;
                    hits += count;
                    next
                })
                .collect();
            (Value::Array(mapped), redacted, hits)
        }
        Value::Object(map) => {
            let mut redacted = false;
            let mut hits = 0;
            let mut next = serde_json::Map::new();
            for (key, item) in map {
                let (value, did, count) = redact_value(item, rules);
                redacted |= did;
                hits += count;
                next.insert(key.clone(), value);
            }
            (Value::Object(next), redacted, hits)
        }
        other => (other.clone(), false, 0),
    }
}

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

fn is_email_local(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-')
}

fn is_email_domain(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-')
}

fn email_span(chars: &[char], at: usize) -> Option<(usize, usize)> {
    let mut start = at;
    while start > 0 && is_email_local(chars[start - 1]) {
        start -= 1;
    }
    if start == at {
        return None;
    }
    let mut end = at + 1;
    while end < chars.len() && is_email_domain(chars[end]) {
        end += 1;
    }
    let domain = &chars[at + 1..end];
    if domain.len() < 3 || !domain.contains(&'.') {
        return None;
    }
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_builtin_patterns() {
        let rules = RedactionRules::default();
        let outcome = redact("联系我 13812345678 或 a@b.com", &rules);
        assert!(outcome.redacted);
        assert!(!outcome.text.contains("13812345678"));
        assert!(!outcome.text.contains("a@b.com"));
    }

    #[test]
    fn masks_custom_terms() {
        let rules = RedactionRules {
            enabled: true,
            terms: vec!["天河计划".to_string()],
            mask: DEFAULT_MASK.to_string(),
        };
        let outcome = redact("天河计划进入第二阶段", &rules);
        assert!(outcome.redacted);
        assert!(!outcome.text.contains("天河计划"));
    }

    #[test]
    fn disabled_rules_pass_through() {
        let rules = RedactionRules {
            enabled: false,
            ..RedactionRules::default()
        };
        let outcome = redact("13812345678", &rules);
        assert_eq!(outcome.text, "13812345678");
        assert!(!outcome.redacted);
    }
}
