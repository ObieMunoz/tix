use std::io::{self, BufRead, Read};

use anyhow::{Result, bail};

use crate::config::Config;
use crate::git::Git;
use crate::hooks::pre_commit::{first_matching_pattern, format_source};

pub fn run(_args: &[String]) -> Result<()> {
    let git = Git::current();
    let Ok(repo_root) = git.repo_root() else {
        return Ok(());
    };
    let cfg = Config::load(Some(&repo_root))?;
    if cfg.branches.protected.is_empty() {
        return Ok(());
    }

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let violations = check_lines(buf.as_bytes(), &cfg.branches.protected)?;
    if violations.is_empty() {
        return Ok(());
    }

    let source = cfg
        .source("branches.protected")
        .map(format_source)
        .unwrap_or("?");
    let mut msg = format!(
        "push blocked: {} protected ref(s) (from {source}):\n",
        violations.len()
    );
    for (b, p) in &violations {
        msg.push_str(&format!("  - {b} matches '{p}'\n"));
    }
    msg.push_str("pass --no-verify to bypass.");
    bail!(msg);
}

pub fn check_lines<R: Read>(reader: R, patterns: &[String]) -> Result<Vec<(String, String)>> {
    let mut buf = io::BufReader::new(reader);
    let mut line = String::new();
    let mut out = Vec::new();
    loop {
        line.clear();
        if buf.read_line(&mut line)? == 0 {
            break;
        }
        if let Some(violation) = check_one_line(&line, patterns) {
            out.push(violation);
        }
    }
    Ok(out)
}

fn check_one_line(line: &str, patterns: &[String]) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let local_ref = parts[0];
    let local_sha = parts[1];

    if local_sha.chars().all(|c| c == '0') {
        return None;
    }

    let local_branch = local_ref.strip_prefix("refs/heads/").unwrap_or(local_ref);
    first_matching_pattern(local_branch, patterns).map(|p| (local_branch.to_string(), p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protected() -> Vec<String> {
        vec![
            "main".to_string(),
            "master".to_string(),
            "release/*".to_string(),
        ]
    }

    #[test]
    fn flags_protected_branch_push() {
        let line = "refs/heads/main aaaa refs/heads/main bbbb\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "main");
    }

    #[test]
    fn allows_unprotected_branch_push() {
        let line = "refs/heads/feature/x aaaa refs/heads/feature/x bbbb\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn allows_branch_deletion() {
        // Deletion: local_sha is all zeros.
        let line = "(delete) 0000000000000000000000000000000000000000 refs/heads/main bbbb\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn flags_glob_match() {
        let line = "refs/heads/release/1.0 aaaa refs/heads/release/1.0 bbbb\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].1, "release/*");
    }

    #[test]
    fn allows_branch_outside_glob_segment() {
        let line = "refs/heads/release/1.0/rc1 aaaa refs/heads/release/1.0/rc1 bbbb\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn flags_one_of_multiple_refs() {
        let input = "refs/heads/feature/x aaaa refs/heads/feature/x bbbb\n\
                     refs/heads/main cccc refs/heads/main dddd\n";
        let v = check_lines(input.as_bytes(), &protected()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "main");
    }

    #[test]
    fn ignores_short_lines() {
        let line = "incomplete line\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert!(v.is_empty());
    }
}
