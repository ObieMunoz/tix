use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use regex::Regex;

use crate::config::Config;
use crate::git::Git;
use crate::state::{BranchEntry, State};
use crate::util::prompt;

const MAX_RETRIES: usize = 3;

pub fn run() -> Result<()> {
    let git = Git::current();
    let Ok(repo_root) = git.repo_root() else {
        return Ok(());
    };
    let Ok(branch) = git.current_branch() else {
        return Ok(());
    };
    let git_dir = git.git_dir()?;
    let state = State::load(&git_dir)?;
    if state.get_branch(&branch).is_some() {
        return Ok(());
    }

    let cfg = Config::load(Some(&repo_root))?;
    let mut prompter = RealPrompter;
    prompt_and_persist(&git_dir, &branch, &cfg, &mut prompter)
}

pub trait Prompter {
    fn line(&mut self, question: &str) -> Result<String>;
}

struct RealPrompter;

impl Prompter for RealPrompter {
    fn line(&mut self, q: &str) -> Result<String> {
        prompt::line(q)
    }
}

pub fn prompt_and_persist<P: Prompter>(
    git_dir: &Path,
    branch: &str,
    cfg: &Config,
    p: &mut P,
) -> Result<()> {
    let pattern = Regex::new(&cfg.ticket.pattern)
        .with_context(|| format!("invalid ticket.pattern: {}", cfg.ticket.pattern))?;

    for attempt in 0..MAX_RETRIES {
        let q = format!("Ticket for branch '{branch}' (blank for no-ticket):");
        let input = p.line(&q)?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            persist(git_dir, branch, None)?;
            println!("(no-ticket mode for {branch})");
            return Ok(());
        }
        if pattern.is_match(trimmed) {
            persist(git_dir, branch, Some(trimmed.to_string()))?;
            println!("set ticket on {branch}: {trimmed}");
            return Ok(());
        }
        eprintln!(
            "invalid: {trimmed:?} does not match pattern {} (attempt {}/{MAX_RETRIES})",
            cfg.ticket.pattern,
            attempt + 1,
        );
    }
    bail!("ticket prompt: invalid input after {MAX_RETRIES} attempts")
}

fn persist(git_dir: &Path, branch: &str, ticket: Option<String>) -> Result<()> {
    let mut state = State::load(git_dir)?;
    let prev = state.get_branch(branch).cloned();
    state.set_branch(
        branch,
        BranchEntry {
            ticket,
            set_at: Utc::now(),
            amended_through: prev.and_then(|e| e.amended_through),
        },
    );
    state.save(git_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPrompter {
        responses: Vec<String>,
        prompts: Vec<String>,
    }

    impl MockPrompter {
        fn new(responses: &[&str]) -> Self {
            Self {
                responses: responses.iter().map(|s| s.to_string()).collect(),
                prompts: Vec::new(),
            }
        }
    }

    impl Prompter for MockPrompter {
        fn line(&mut self, q: &str) -> Result<String> {
            self.prompts.push(q.to_string());
            if self.responses.is_empty() {
                bail!("no more mock responses");
            }
            Ok(self.responses.remove(0))
        }
    }

    fn fixture() -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tix")).unwrap();
        (dir, Config::defaults())
    }

    #[test]
    fn valid_ticket_persists_and_returns_ok() {
        let (dir, cfg) = fixture();
        let mut p = MockPrompter::new(&["POD-1234"]);
        prompt_and_persist(dir.path(), "main", &cfg, &mut p).unwrap();
        let state = State::load(dir.path()).unwrap();
        assert_eq!(
            state.get_branch("main").unwrap().ticket.as_deref(),
            Some("POD-1234")
        );
    }

    #[test]
    fn empty_input_persists_no_ticket_mode() {
        let (dir, cfg) = fixture();
        let mut p = MockPrompter::new(&[""]);
        prompt_and_persist(dir.path(), "main", &cfg, &mut p).unwrap();
        let state = State::load(dir.path()).unwrap();
        assert!(state.get_branch("main").unwrap().ticket.is_none());
    }

    #[test]
    fn whitespace_only_input_treated_as_empty() {
        let (dir, cfg) = fixture();
        let mut p = MockPrompter::new(&["   "]);
        prompt_and_persist(dir.path(), "main", &cfg, &mut p).unwrap();
        let state = State::load(dir.path()).unwrap();
        assert!(state.get_branch("main").unwrap().ticket.is_none());
    }

    #[test]
    fn invalid_then_valid_succeeds_within_retry_budget() {
        let (dir, cfg) = fixture();
        let mut p = MockPrompter::new(&["nope", "still-bad", "POD-9"]);
        prompt_and_persist(dir.path(), "main", &cfg, &mut p).unwrap();
        let state = State::load(dir.path()).unwrap();
        assert_eq!(
            state.get_branch("main").unwrap().ticket.as_deref(),
            Some("POD-9")
        );
    }

    #[test]
    fn three_invalid_attempts_errors_out_without_persisting() {
        let (dir, cfg) = fixture();
        let mut p = MockPrompter::new(&["bad1", "bad2", "bad3"]);
        let err = prompt_and_persist(dir.path(), "main", &cfg, &mut p).unwrap_err();
        assert!(format!("{err:#}").contains("invalid input"));
        let state = State::load(dir.path()).unwrap();
        assert!(state.get_branch("main").is_none());
    }

    #[test]
    fn prompt_includes_branch_name_and_no_ticket_hint() {
        let (dir, cfg) = fixture();
        let mut p = MockPrompter::new(&[""]);
        prompt_and_persist(dir.path(), "feature/x", &cfg, &mut p).unwrap();
        let q = &p.prompts[0];
        assert!(q.contains("feature/x"));
        assert!(q.contains("blank"));
    }
}
