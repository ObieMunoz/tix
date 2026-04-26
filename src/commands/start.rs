use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use regex::Regex;

use crate::config::Config;
use crate::git::Git;
use crate::state::{BranchEntry, State};
use crate::util::{slug, ticket};

const SLUG_MAX_LEN: usize = 50;

pub fn run(ticket_id: &str, description: Option<&str>, base_override: Option<&str>) -> Result<()> {
    let git = Git::current();
    let repo_root = git.repo_root().map_err(|_| anyhow!("not in a git repo"))?;

    let cfg = Config::load(Some(&repo_root))?;
    let pattern = Regex::new(&cfg.ticket.pattern)
        .with_context(|| format!("invalid ticket.pattern: {}", cfg.ticket.pattern))?;
    ticket::validate(ticket_id, &pattern)?;

    if !git.is_clean()? {
        bail!("working tree is dirty; commit or stash before `tix start`");
    }

    let base = base_override.unwrap_or(&cfg.branches.default_base);
    git.fetch("origin", base).with_context(|| {
        format!("fetching origin/{base} (branching off a stale base would be misleading)")
    })?;

    let new_branch = compose_branch_name(&cfg.branches.start_prefix, ticket_id, description);

    if git.run(&["rev-parse", "--verify", &new_branch]).is_ok() {
        bail!("branch '{new_branch}' already exists");
    }

    git.run(&["checkout", "-b", &new_branch, &format!("origin/{base}")])
        .with_context(|| format!("creating branch '{new_branch}' off origin/{base}"))?;

    let git_dir = git.git_dir()?;
    let mut state = State::load(&git_dir)?;
    state.set_branch(
        new_branch.clone(),
        BranchEntry {
            ticket: Some(ticket_id.to_string()),
            set_at: Utc::now(),
            amended_through: None,
        },
    );
    state.save(&git_dir)?;

    let head = git.current_commit()?;
    let short = &head[..7.min(head.len())];
    println!("Started {new_branch} off {base} @ {short} with ticket {ticket_id}");
    Ok(())
}

fn compose_branch_name(prefix: &str, ticket: &str, description: Option<&str>) -> String {
    match description.and_then(|d| {
        let s = slug::slugify(d, SLUG_MAX_LEN);
        (!s.is_empty()).then_some(s)
    }) {
        Some(slug_str) => format!("{prefix}/{ticket}-{slug_str}"),
        None => format!("{prefix}/{ticket}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_name_without_description() {
        assert_eq!(
            compose_branch_name("feature", "POD-1", None),
            "feature/POD-1"
        );
    }

    #[test]
    fn branch_name_with_description_is_slugified() {
        assert_eq!(
            compose_branch_name("feature", "POD-1", Some("Fix Login Bug")),
            "feature/POD-1-fix-login-bug"
        );
    }

    #[test]
    fn description_that_slugifies_to_empty_falls_back_to_no_slug() {
        assert_eq!(
            compose_branch_name("feature", "POD-1", Some("---")),
            "feature/POD-1"
        );
    }

    #[test]
    fn long_description_is_truncated_to_max_len() {
        let long = "a".repeat(200);
        let name = compose_branch_name("feature", "POD-1", Some(&long));
        // "feature/POD-1-" + 50 a's
        assert_eq!(name, format!("feature/POD-1-{}", "a".repeat(SLUG_MAX_LEN)));
    }

    #[test]
    fn custom_start_prefix() {
        assert_eq!(
            compose_branch_name("bug", "POD-1", Some("foo")),
            "bug/POD-1-foo"
        );
    }
}
