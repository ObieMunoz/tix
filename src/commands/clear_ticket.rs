use anyhow::{Context, Result, anyhow};
use chrono::Utc;

use crate::git::Git;
use crate::state::{BranchEntry, State};

pub fn run() -> Result<()> {
    let git = Git::current();
    git.repo_root().map_err(|_| anyhow!("not in a git repo"))?;
    let branch = git
        .current_branch()
        .context("could not determine current branch")?;

    let git_dir = git.git_dir()?;
    let mut state = State::load(&git_dir)?;
    let prev = state.get_branch(&branch).cloned();

    state.set_branch(
        branch.clone(),
        BranchEntry {
            ticket: None,
            set_at: Utc::now(),
            amended_through: prev.as_ref().and_then(|e| e.amended_through.clone()),
        },
    );
    state.save(&git_dir)?;

    println!("cleared ticket on {branch} (no-ticket mode)");
    Ok(())
}
