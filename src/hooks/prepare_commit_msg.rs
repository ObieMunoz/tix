use std::path::Path;

use anyhow::{Context, Result, anyhow};
use regex::Regex;

use crate::config::Config;
use crate::git::Git;
use crate::state::State;
use crate::util::ticket;

pub fn run(args: &[String]) -> Result<()> {
    let msg_path = args
        .first()
        .ok_or_else(|| anyhow!("prepare-commit-msg requires a commit message file argument"))?;
    let source = args.get(1).map(String::as_str).unwrap_or("");

    if matches!(source, "merge" | "squash") {
        return Ok(());
    }

    let git = Git::current();
    let Ok(repo_root) = git.repo_root() else {
        return Ok(());
    };
    let Ok(branch) = git.current_branch() else {
        return Ok(());
    };
    let git_dir = git.git_dir()?;
    let state = State::load(&git_dir)?;

    let Some(ticket_id) = state.get_branch(&branch).and_then(|e| e.ticket.clone()) else {
        return Ok(());
    };

    let cfg = Config::load(Some(&repo_root))?;
    let pattern = Regex::new(&cfg.ticket.pattern)
        .with_context(|| format!("invalid ticket.pattern: {}", cfg.ticket.pattern))?;

    let path = Path::new(msg_path);
    let original = std::fs::read_to_string(path)
        .with_context(|| format!("reading commit message file: {msg_path}"))?;

    if let Some(line) = first_real_line(&original)
        && ticket::extract_prefix(line, &pattern).is_some()
    {
        return Ok(());
    }

    let prefixed = apply_prefix(&original, &cfg.ticket.prefix_format, &ticket_id);
    std::fs::write(path, &prefixed)
        .with_context(|| format!("writing commit message file: {msg_path}"))?;
    Ok(())
}

fn is_real_line(line: &str) -> bool {
    !line.trim_start().starts_with('#') && !line.trim().is_empty()
}

fn first_real_line(content: &str) -> Option<&str> {
    content.lines().find(|l| is_real_line(l))
}

fn apply_prefix(content: &str, prefix_format: &str, ticket: &str) -> String {
    let format_with = |message: &str| -> String {
        prefix_format
            .replace("{ticket}", ticket)
            .replace("{message}", message)
    };

    let parts: Vec<&str> = content.split('\n').collect();
    let first_real = parts.iter().position(|p| is_real_line(p));

    match first_real {
        Some(i) => {
            let mut out: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
            out[i] = format_with(parts[i]);
            out.join("\n")
        }
        None => {
            // No real line — prepend a new one with `{ticket} ` so the
            // user types the message right after the prefix.
            let new_first = format_with("");
            if content.is_empty() {
                new_first
            } else {
                format!("{new_first}\n{content}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_simple_message() {
        let out = apply_prefix("fix bug\n", "{ticket} {message}", "POD-1");
        assert_eq!(out, "POD-1 fix bug\n");
    }

    #[test]
    fn prefixes_only_first_real_line() {
        let out = apply_prefix("fix bug\n\nbody text\n", "{ticket} {message}", "POD-1");
        assert_eq!(out, "POD-1 fix bug\n\nbody text\n");
    }

    #[test]
    fn skips_leading_comments_and_blank_lines() {
        let out = apply_prefix(
            "\n# Please enter the commit message\n",
            "{ticket} {message}",
            "POD-1",
        );
        // First non-comment, non-empty line must hold the prefix.
        assert!(out.contains("POD-1 "));
    }

    #[test]
    fn empty_file_yields_just_the_prefix() {
        let out = apply_prefix("", "{ticket} {message}", "POD-1");
        assert_eq!(out, "POD-1 ");
    }

    #[test]
    fn extract_prefix_detects_existing_ticket_in_first_real_line() {
        let pat = Regex::new(r"^[A-Z]+-\d+$").unwrap();
        assert!(ticket::extract_prefix("POD-2 something", &pat).is_some());
        assert!(ticket::extract_prefix("# comment\nfix it", &pat).is_none());
    }

    #[test]
    fn first_real_line_skips_comments() {
        assert_eq!(
            first_real_line("\n# comment\nactual line\n"),
            Some("actual line")
        );
        assert_eq!(first_real_line("# only comments\n"), None);
        assert_eq!(first_real_line("\n\n\n"), None);
    }
}
