use anyhow::Result;

use crate::config::{Config, Source};
use crate::git::Git;
use crate::state::State;
use crate::util::glob;

const SOURCE_KEYS: &[(&str, &str)] = &[
    ("ticket.pattern", "ticket pattern"),
    ("ticket.prefix_format", "prefix format"),
    ("branches.default_base", "default base"),
    ("branches.protected", "protected list"),
    ("branches.naming_pattern", "naming pattern"),
    ("branches.naming_enforcement", "naming enforcement"),
    ("integrations.ticket_url_template", "ticket URL template"),
    ("integrations.pr_provider", "PR provider"),
];

pub fn run() -> Result<()> {
    let git = Git::current();
    let repo_root = git.repo_root().ok();
    let branch = git.current_branch().ok();
    let cfg = Config::load(repo_root.as_deref())?;

    match (&repo_root, &branch) {
        (Some(_), Some(b)) => println!("Branch: {b}"),
        (Some(_), None) => println!("Branch: (detached HEAD or no commits yet)"),
        (None, _) => println!("Branch: (not in a git repo)"),
    }

    if let (Some(_), Some(branch_name)) = (&repo_root, &branch) {
        let git_dir = git.git_dir()?;
        let state = State::load(&git_dir)?;
        let line = match state.get_branch(branch_name) {
            Some(entry) => match &entry.ticket {
                Some(t) => t.clone(),
                None => "(no-ticket mode)".to_string(),
            },
            None => "(not set — first commit will prompt)".to_string(),
        };
        println!("Ticket: {line}");
    }

    if repo_root.is_some() {
        println!("Protected branches:");
        for pattern in &cfg.branches.protected {
            let suffix = match &branch {
                Some(b) if glob::matches(pattern, b) => "  ← current",
                _ => "",
            };
            println!("  - {pattern}{suffix}");
        }
        println!("Base: {}", cfg.branches.default_base);
    }

    println!("Config sources:");
    for (key, label) in SOURCE_KEYS {
        if let Some(src) = cfg.source(key) {
            let s = match src {
                Source::Default => "default",
                Source::Global => "global",
                Source::Repo => "repo",
            };
            println!("  - {label} ({key}): {s}");
        }
    }

    Ok(())
}
