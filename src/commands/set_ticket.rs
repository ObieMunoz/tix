use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use regex::Regex;

use crate::config::Config;
use crate::git::Git;
use crate::state::{BranchEntry, State};
use crate::util::ticket;

pub fn run(ticket_id: &str, _force: bool) -> Result<()> {
    let git = Git::current();
    let repo_root = git.repo_root().map_err(|_| anyhow!("not in a git repo"))?;
    let branch = git
        .current_branch()
        .context("could not determine current branch (detached HEAD?)")?;

    let cfg = Config::load(Some(&repo_root))?;
    let pattern = Regex::new(&cfg.ticket.pattern)
        .with_context(|| format!("invalid ticket.pattern: {}", cfg.ticket.pattern))?;
    ticket::validate(ticket_id, &pattern)?;

    let git_dir = git.git_dir()?;
    let mut state = State::load(&git_dir)?;
    let prev = state.get_branch(&branch).cloned();

    state.set_branch(
        branch.clone(),
        BranchEntry {
            ticket: Some(ticket_id.to_string()),
            set_at: Utc::now(),
            amended_through: prev.as_ref().and_then(|e| e.amended_through.clone()),
        },
    );
    state.save(&git_dir)?;

    match prev.and_then(|e| e.ticket) {
        Some(p) if p == ticket_id => println!("ticket {ticket_id} already set on {branch}"),
        Some(p) => println!("set ticket on {branch}: {p} \u{2192} {ticket_id}"),
        None => println!("set ticket on {branch}: {ticket_id}"),
    }
    Ok(())
}
