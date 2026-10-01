use crate::error::{AppError, Result};

pub fn model_name(name: &str, short_code: &str) -> String {
    format!("{}({})", name.trim(), short_code.trim())
}

pub fn validate_short_code(short_code: &str) -> Result<()> {
    if short_code.is_empty()
        || short_code.len() > 24
        || !short_code
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, b'-' | b'_'))
    {
        return Err(AppError::validation(
            "Provider 简写须为 1 到 24 位小写字母、数字、连字符或下划线",
        ));
    }
    Ok(())
}

/// Derive a short code candidate from a provider display name.
pub fn short_code_base(name: &str) -> String {
    let base: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "provider" } else { base };
    base.chars().take(18).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_short_codes() {
        assert!(validate_short_code("work-1").is_ok());
        for code in ["", "KK", "has space", "bad/code", "has(parentheses)"] {
            assert!(validate_short_code(code).is_err(), "{code}");
        }
        assert!(validate_short_code(&"a".repeat(25)).is_err());
    }

    #[test]
    fn derives_short_code_bases() {
        assert_eq!(short_code_base("Work account"), "work-account");
        assert_eq!(short_code_base("###"), "provider");
        assert_eq!(model_name(" gpt ", " kk "), "gpt(kk)");
    }
}
