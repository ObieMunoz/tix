use anyhow::{Result, anyhow};
use regex::Regex;

pub fn validate(s: &str, pattern: &Regex) -> Result<()> {
    if pattern.is_match(s) {
        Ok(())
    } else {
        Err(anyhow!(
            "ticket {s:?} does not match pattern {}",
            pattern.as_str()
        ))
    }
}

pub fn extract_prefix<'a>(message: &'a str, pattern: &Regex) -> Option<&'a str> {
    let first = message.split_whitespace().next()?;
    pattern.is_match(first).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat() -> Regex {
        Regex::new(r"^[A-Z]+-\d+$").unwrap()
    }

    #[test]
    fn validate_accepts_well_formed_ticket() {
        assert!(validate("POD-1234", &pat()).is_ok());
        assert!(validate("DPT-1", &pat()).is_ok());
    }

    #[test]
    fn validate_rejects_lowercase_or_missing_dash() {
        assert!(validate("pod-1", &pat()).is_err());
        assert!(validate("POD1", &pat()).is_err());
        assert!(validate("", &pat()).is_err());
    }

    #[test]
    fn validate_error_includes_input_and_pattern() {
        let err = validate("nope", &pat()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("nope"), "expected input in error: {msg}");
        assert!(
            msg.contains("[A-Z]"),
            "expected pattern hint in error: {msg}"
        );
    }

    #[test]
    fn extract_prefix_returns_first_matching_token() {
        assert_eq!(extract_prefix("POD-1 fix the thing", &pat()), Some("POD-1"));
    }

    #[test]
    fn extract_prefix_skips_leading_whitespace() {
        assert_eq!(extract_prefix("   POD-1 leading", &pat()), Some("POD-1"));
    }

    #[test]
    fn extract_prefix_returns_none_when_first_token_doesnt_match() {
        assert_eq!(extract_prefix("fix bug POD-1", &pat()), None);
    }

    #[test]
    fn extract_prefix_returns_none_for_empty() {
        assert_eq!(extract_prefix("", &pat()), None);
        assert_eq!(extract_prefix("   ", &pat()), None);
    }
}
