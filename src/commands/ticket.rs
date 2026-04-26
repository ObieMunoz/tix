use std::process::Command as ProcCommand;

use anyhow::{Context, Result, anyhow, bail};

use crate::config::Config;
use crate::git::Git;
use crate::state::State;

pub fn run(open_in_browser: bool) -> Result<()> {
    let git = Git::current();
    let repo_root = git.repo_root().map_err(|_| anyhow!("not in a git repo"))?;
    let branch = git
        .current_branch()
        .context("could not determine current branch")?;
    let cfg = Config::load(Some(&repo_root))?;

    if cfg.integrations.ticket_url_template.is_empty() {
        bail!(
            "integrations.ticket_url_template is empty; set it via `tix config set integrations.ticket_url_template \"https://your.atlassian.net/browse/{{ticket}}\"`"
        );
    }

    let git_dir = git.git_dir()?;
    let state = State::load(&git_dir)?;
    let ticket = match state.get_branch(&branch) {
        Some(entry) => match &entry.ticket {
            Some(t) => t.clone(),
            None => bail!("branch '{branch}' is in no-ticket mode; nothing to open"),
        },
        None => bail!("branch '{branch}' has no ticket — run `tix set-ticket <ID>` first"),
    };

    let url = cfg
        .integrations
        .ticket_url_template
        .replace("{ticket}", &ticket);

    if open_in_browser {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        ProcCommand::new(opener)
            .arg(&url)
            .status()
            .with_context(|| format!("invoking `{opener}`"))?;
    } else {
        println!("{url}");
    }
    Ok(())
}
