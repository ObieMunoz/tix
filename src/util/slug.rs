pub fn slugify(s: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            for ch in c.to_ascii_lowercase().to_string().chars() {
                out.push(ch);
            }
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > max_len {
        out.truncate(max_len);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_dashes_spaces() {
        assert_eq!(slugify("Hello World", 50), "hello-world");
    }

    #[test]
    fn trims_leading_and_trailing_dashes() {
        assert_eq!(slugify("  Trim Me  ", 50), "trim-me");
        assert_eq!(slugify("---Edges---", 50), "edges");
    }

    #[test]
    fn collapses_consecutive_non_alphanum() {
        assert_eq!(slugify("fix login (#123)", 50), "fix-login-123");
        assert_eq!(slugify("a___b", 50), "a-b");
    }

    #[test]
    fn non_ascii_becomes_dash() {
        assert_eq!(slugify("café", 50), "caf");
        assert_eq!(slugify("über cool", 50), "ber-cool");
    }

    #[test]
    fn truncates_to_max_len_and_re_trims_trailing_dash() {
        assert_eq!(slugify("hello world", 6), "hello");
        assert_eq!(slugify("hello-world", 8), "hello-wo");
    }

    #[test]
    fn empty_input_yields_empty() {
        assert_eq!(slugify("", 50), "");
        assert_eq!(slugify("   ", 50), "");
        assert_eq!(slugify("---", 50), "");
    }

    #[test]
    fn already_slugged_passes_through() {
        assert_eq!(slugify("already-slugged", 50), "already-slugged");
    }
}
