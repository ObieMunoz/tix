use std::io::{self, BufRead, Read};

use anyhow::{Result, bail};

use crate::config::Config;
use crate::git::Git;
use crate::hooks::pre_commit::{
    NamingResult, check_branch_naming, first_matching_pattern, format_source,
};

pub fn run(_args: &[String]) -> Result<()> {
    let git = Git::current();
    let Ok(repo_root) = git.repo_root() else {
        return Ok(());
    };
    let cfg = Config::load(Some(&repo_root))?;

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let pushed_refs = pushed_branches(buf.as_bytes())?;

    if !cfg.branches.protected.is_empty() {
        let violations: Vec<(String, String)> = pushed_refs
            .iter()
            .filter_map(|b| {
                first_matching_pattern(b, &cfg.branches.protected).map(|p| (b.clone(), p))
            })
            .collect();
        if !violations.is_empty() {
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
    }

    let mut blocking = Vec::new();
    for b in &pushed_refs {
        match check_branch_naming(b, &cfg)? {
            NamingResult::Ok => {}
            NamingResult::Warn(m) => eprintln!("warning: {m}"),
            NamingResult::Block(m) => blocking.push(m),
        }
    }
    if !blocking.is_empty() {
        bail!("{}", blocking.join("\n"));
    }

    warn_if_stale_base(&git, &cfg, &pushed_refs);

    Ok(())
}

fn warn_if_stale_base(git: &Git, cfg: &Config, pushed_refs: &[String]) {
    if cfg.push.stale_warn_threshold == 0 || pushed_refs.is_empty() {
        return;
    }
    let base = &cfg.branches.default_base;
    if git.fetch("origin", base).is_err() {
        return;
    }
    let remote_ref = format!("origin/{base}");
    for branch in pushed_refs {
        if branch == base {
            continue;
        }
        let range = format!("{branch}..{remote_ref}");
        let Ok(out) = git.run(&["rev-list", "--count", &range]) else {
            continue;
        };
        let Ok(count) = out.parse::<u32>() else {
            continue;
        };
        if count > cfg.push.stale_warn_threshold {
            eprintln!(
                "warning: '{branch}' is {count} commits behind {remote_ref}; consider `git rebase {remote_ref}`"
            );
        }
    }
}

fn pushed_branches<R: Read>(reader: R) -> Result<Vec<String>> {
    let mut buf = io::BufReader::new(reader);
    let mut line = String::new();
    let mut out = Vec::new();
    loop {
        line.clear();
        if buf.read_line(&mut line)? == 0 {
            break;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let local_ref = parts[0];
        let local_sha = parts[1];
        if local_sha.chars().all(|c| c == '0') {
            continue;
        }
        // Only branch refs participate in branch-naming and
        // branch-protection checks. Tag refs (refs/tags/*) and other
        // ref types are forwarded by git via the same pre-push protocol
        // but should not be treated as branches.
        let Some(local_branch) = local_ref.strip_prefix("refs/heads/") else {
            continue;
        };
        out.push(local_branch.to_string());
    }
    Ok(out)
}

pub fn check_lines<R: Read>(reader: R, patterns: &[String]) -> Result<Vec<(String, String)>> {
    let branches = pushed_branches(reader)?;
    Ok(branches
        .into_iter()
        .filter_map(|b| first_matching_pattern(&b, patterns).map(|p| (b, p)))
        .collect())
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

    #[test]
    fn tag_refs_are_skipped_for_branch_checks() {
        // Pushing `refs/tags/v0.1.1` must not be treated as a branch
        // — branch naming and branch protection don't apply to tags.
        let line = "refs/tags/v0.1.1 aaaa refs/tags/v0.1.1 bbbb\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert!(v.is_empty(), "tag refs should not match branch protection");
    }

    #[test]
    fn mixed_branch_and_tag_only_returns_branch() {
        let input = "refs/tags/v1.0 aaaa refs/tags/v1.0 bbbb\n\
                     refs/heads/main cccc refs/heads/main dddd\n";
        let v = check_lines(input.as_bytes(), &protected()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "main");
    }

    #[test]
    fn refs_notes_and_other_namespaces_are_ignored() {
        let line = "refs/notes/commits aaaa refs/notes/commits bbbb\n";
        let v = check_lines(line.as_bytes(), &protected()).unwrap();
        assert!(v.is_empty());
    }
}
