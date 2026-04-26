use regex::Regex;

pub fn matches(pattern: &str, branch: &str) -> bool {
    if pattern.contains("**") {
        return false;
    }
    let mut re = String::with_capacity(pattern.len() + 4);
    re.push('^');
    for c in pattern.chars() {
        match c {
            '*' => re.push_str("[^/]*"),
            '/' => re.push('/'),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' => re.push(c),
            other => {
                re.push('\\');
                re.push(other);
            }
        }
    }
    re.push('$');
    Regex::new(&re).map(|r| r.is_match(branch)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_pattern_matches_exact_branch() {
        assert!(matches("main", "main"));
        assert!(!matches("main", "main2"));
        assert!(!matches("main", "feature/main"));
    }

    #[test]
    fn star_matches_within_a_segment() {
        assert!(matches("release/*", "release/1.0"));
        assert!(matches("release/*", "release/rc1"));
    }

    #[test]
    fn star_does_not_cross_slash() {
        assert!(!matches("release/*", "release/1.0/rc1"));
        assert!(!matches("release/*", "release"));
    }

    #[test]
    fn star_matches_prefix_within_segment() {
        assert!(matches("release/rc-*", "release/rc-1"));
        assert!(matches("release/rc-*", "release/rc-final"));
        assert!(!matches("release/rc-*", "release/v1"));
    }

    #[test]
    fn no_doublestar_support() {
        assert!(!matches("release/**", "release/1.0/rc1"));
    }

    #[test]
    fn segment_count_must_match() {
        assert!(!matches("a/*/c", "a/c"));
        assert!(matches("a/*/c", "a/b/c"));
        assert!(!matches("a/*/c", "a/b/c/d"));
    }
}
