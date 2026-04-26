use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use regex::Regex;
use tempfile::NamedTempFile;

use crate::config::Config;
use crate::git::Git;
use crate::state::{BranchEntry, State};
use crate::util::{prompt, ticket};

struct Candidate {
    sha: String,
    subject: String,
    needs_rewrite: bool,
}

pub fn run(ticket_id: &str, force: bool, yes: bool) -> Result<()> {
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
    let prev_ticket = State::load(&git_dir)?
        .get_branch(&branch)
        .and_then(|e| e.ticket.clone());
    persist_ticket(&git_dir, &branch, Some(ticket_id.to_string()), None)?;
    match prev_ticket.as_deref() {
        Some(p) if p == ticket_id => println!("ticket {ticket_id} already set on {branch}"),
        Some(p) => println!("set ticket on {branch}: {p} \u{2192} {ticket_id}"),
        None => println!("set ticket on {branch}: {ticket_id}"),
    }

    let candidates = match compute_candidates(&git, &branch, &cfg.branches.default_base, &pattern) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("note: skipped retroactive amend ({e})");
            return Ok(());
        }
    };
    let to_rewrite_count = candidates.iter().filter(|c| c.needs_rewrite).count();
    if to_rewrite_count == 0 {
        return Ok(());
    }

    let on_remote = find_on_remote(&git, &candidates)?;
    if !on_remote.is_empty() {
        if !force {
            bail!(
                "refusing to amend: {} commit(s) are reachable from remote refs. Use --force to rewrite anyway (will require a force-push).",
                on_remote.len()
            );
        }
        if !yes
            && !prompt::confirm(
                &format!(
                    "WARNING: {} commit(s) on remote will need force-push. Proceed?",
                    on_remote.len()
                ),
                false,
            )?
        {
            return Ok(());
        }
    }

    println!("Found {to_rewrite_count} unprefixed unpushed commit(s):");
    for c in candidates.iter().filter(|c| c.needs_rewrite) {
        let new_subj = format_prefix(&cfg.ticket.prefix_format, ticket_id, &c.subject);
        println!(
            "  {} {} \u{2192} {}",
            short_sha(&c.sha),
            c.subject,
            new_subj
        );
    }

    if !yes {
        match prompt::confirm(&format!("amend {to_rewrite_count} commits?"), false) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(e) => {
                eprintln!("note: skipped retroactive amend ({e})");
                return Ok(());
            }
        }
    }

    let new_head = retroactive_amend(&git, &candidates, ticket_id, &cfg.ticket.prefix_format)?;
    persist_amended_through(&git_dir, &branch, &new_head)?;
    println!(
        "amended {to_rewrite_count} commits; new HEAD = {}",
        short_sha(&new_head)
    );
    Ok(())
}

fn persist_ticket(
    git_dir: &std::path::Path,
    branch: &str,
    ticket: Option<String>,
    amended_through_override: Option<Option<String>>,
) -> Result<()> {
    let mut state = State::load(git_dir)?;
    let prev = state.get_branch(branch).cloned();
    let amended_through = match amended_through_override {
        Some(v) => v,
        None => prev.as_ref().and_then(|e| e.amended_through.clone()),
    };
    state.set_branch(
        branch,
        BranchEntry {
            ticket,
            set_at: Utc::now(),
            amended_through,
        },
    );
    state.save(git_dir)
}

fn persist_amended_through(git_dir: &std::path::Path, branch: &str, sha: &str) -> Result<()> {
    let mut state = State::load(git_dir)?;
    if let Some(entry) = state.get_branch(branch).cloned() {
        state.set_branch(
            branch,
            BranchEntry {
                amended_through: Some(sha.to_string()),
                ..entry
            },
        );
        state.save(git_dir)?;
    }
    Ok(())
}

fn compute_candidates(
    git: &Git,
    branch: &str,
    base: &str,
    pattern: &Regex,
) -> Result<Vec<Candidate>> {
    let _ = git.fetch("origin", base);

    let remote_ref = format!("refs/remotes/origin/{base}");
    if git.for_each_ref(&remote_ref)?.is_empty() {
        return Err(anyhow!("origin/{base} not found"));
    }

    let raw = git.run(&["rev-list", "--reverse", branch, &format!("^origin/{base}")])?;
    let shas: Vec<&str> = raw.lines().filter(|l| !l.is_empty()).collect();

    let mut out = Vec::with_capacity(shas.len());
    for sha in shas {
        let subject = git.commit_subject(sha)?;
        let needs_rewrite = ticket::extract_prefix(&subject, pattern).is_none();
        out.push(Candidate {
            sha: sha.to_string(),
            subject,
            needs_rewrite,
        });
    }
    Ok(out)
}

fn find_on_remote(git: &Git, candidates: &[Candidate]) -> Result<Vec<String>> {
    let remote_refs = git.for_each_ref("refs/remotes/")?;
    let mut found = Vec::new();
    for c in candidates.iter().filter(|c| c.needs_rewrite) {
        for r in &remote_refs {
            if git.is_commit_on_remote(&c.sha, r)? {
                found.push(c.sha.clone());
                break;
            }
        }
    }
    Ok(found)
}

fn retroactive_amend(
    git: &Git,
    candidates: &[Candidate],
    ticket: &str,
    prefix_format: &str,
) -> Result<String> {
    let original_head = git.current_commit()?;
    let oldest = &candidates[0].sha;
    let base_sha = git
        .run(&["rev-parse", &format!("{oldest}^")])
        .with_context(|| format!("computing parent of {oldest}"))?;

    if let Err(e) = do_amend(git, &base_sha, candidates, ticket, prefix_format) {
        let _ = git.run(&["cherry-pick", "--abort"]);
        let _ = git.run(&["reset", "--hard", &original_head]);
        return Err(e);
    }
    git.current_commit()
}

fn do_amend(
    git: &Git,
    base_sha: &str,
    candidates: &[Candidate],
    ticket: &str,
    prefix_format: &str,
) -> Result<()> {
    git.run(&["reset", "--hard", base_sha])?;
    for c in candidates {
        git.run(&["cherry-pick", "--allow-empty", &c.sha])
            .with_context(|| format!("cherry-picking {}", short_sha(&c.sha)))?;
        if c.needs_rewrite {
            let full_msg = git.run(&["log", "-1", "--format=%B", "HEAD"])?;
            let new_msg = apply_prefix_to_message(&full_msg, prefix_format, ticket);
            let tmp = NamedTempFile::new()?;
            std::fs::write(tmp.path(), &new_msg)?;
            let path_str = tmp
                .path()
                .to_str()
                .ok_or_else(|| anyhow!("temp path not utf8"))?;
            git.run(&["commit", "--amend", "--allow-empty", "-F", path_str])
                .with_context(|| format!("amending message of {}", short_sha(&c.sha)))?;
        }
    }
    Ok(())
}

fn apply_prefix_to_message(msg: &str, prefix_format: &str, ticket: &str) -> String {
    let mut iter = msg.splitn(2, '\n');
    let first = iter.next().unwrap_or("");
    let rest = iter.next();
    let new_first = prefix_format
        .replace("{ticket}", ticket)
        .replace("{message}", first);
    match rest {
        Some(r) if !r.is_empty() => format!("{new_first}\n{r}"),
        _ => new_first,
    }
}

fn format_prefix(prefix_format: &str, ticket: &str, subject: &str) -> String {
    prefix_format
        .replace("{ticket}", ticket)
        .replace("{message}", subject)
}

fn short_sha(sha: &str) -> &str {
    let n = 7.min(sha.len());
    &sha[..n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_prefix_to_subject_only() {
        assert_eq!(
            apply_prefix_to_message("fix bug", "{ticket} {message}", "POD-1"),
            "POD-1 fix bug"
        );
    }

    #[test]
    fn apply_prefix_preserves_body() {
        let original = "fix bug\n\nlong description\nspans lines\n";
        assert_eq!(
            apply_prefix_to_message(original, "{ticket} {message}", "POD-1"),
            "POD-1 fix bug\n\nlong description\nspans lines\n"
        );
    }

    #[test]
    fn apply_prefix_handles_message_without_trailing_newline() {
        assert_eq!(
            apply_prefix_to_message("fix bug\n\nbody", "{ticket} {message}", "POD-1"),
            "POD-1 fix bug\n\nbody"
        );
    }

    #[test]
    fn short_sha_truncates_to_seven() {
        assert_eq!(short_sha("abcdef1234567890"), "abcdef1");
        assert_eq!(short_sha("abc"), "abc");
    }
}
