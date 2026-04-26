use std::path::Path;
use std::process::Command as ProcCommand;

use assert_cmd::Command;
use tempfile::{NamedTempFile, TempDir};
use tix_git::state::State;

struct Env {
    home: TempDir,
    xdg: TempDir,
    git_global: NamedTempFile,
    repo: TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Self {
            home: tempfile::tempdir().unwrap(),
            xdg: tempfile::tempdir().unwrap(),
            git_global: NamedTempFile::new().unwrap(),
            repo: tempfile::tempdir().unwrap(),
        };
        env.git(&["init", "-b", "main"]);
        env.git(&["config", "user.email", "test@example.com"]);
        env.git(&["config", "user.name", "Test"]);
        env.git(&["config", "commit.gpgsign", "false"]);
        env.git(&["commit", "--allow-empty", "-m", "initial"]);
        env
    }

    fn git(&self, args: &[&str]) {
        let out = ProcCommand::new("git")
            .current_dir(self.repo.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(args)
            .output()
            .unwrap();
        if !out.status.success() {
            panic!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        }
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .current_dir(cwd)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(args)
            .assert()
    }

    fn state(&self) -> State {
        State::load(&self.repo.path().join(".git")).unwrap()
    }
}

#[test]
fn set_ticket_persists_for_current_branch() {
    let env = Env::new();
    env.run(env.repo.path(), &["set-ticket", "POD-1234"])
        .success();
    let entry = env.state().get_branch("main").cloned().unwrap();
    assert_eq!(entry.ticket.as_deref(), Some("POD-1234"));
}

#[test]
fn set_ticket_rejects_input_that_doesnt_match_pattern() {
    let env = Env::new();
    let assert = env
        .run(env.repo.path(), &["set-ticket", "not-a-ticket"])
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not-a-ticket"));
    assert!(
        env.state().get_branch("main").is_none(),
        "state must be unchanged"
    );
}

#[test]
fn set_ticket_overwrites_existing_and_reports_diff() {
    let env = Env::new();
    env.run(env.repo.path(), &["set-ticket", "POD-1"]).success();
    let assert = env.run(env.repo.path(), &["set-ticket", "POD-2"]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("POD-1"));
    assert!(stdout.contains("POD-2"));
    let entry = env.state().get_branch("main").cloned().unwrap();
    assert_eq!(entry.ticket.as_deref(), Some("POD-2"));
}

#[test]
fn set_ticket_idempotent_when_setting_same_value() {
    let env = Env::new();
    env.run(env.repo.path(), &["set-ticket", "POD-1"]).success();
    let assert = env.run(env.repo.path(), &["set-ticket", "POD-1"]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("already set"));
}

#[test]
fn set_ticket_outside_a_repo_errors() {
    let env = Env::new();
    let assert = env.run(env.home.path(), &["set-ticket", "POD-1"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not in a git repo"));
}

#[test]
fn clear_ticket_writes_null_ticket_for_branch() {
    let env = Env::new();
    env.run(env.repo.path(), &["clear-ticket"]).success();
    let entry = env.state().get_branch("main").cloned().unwrap();
    assert!(
        entry.ticket.is_none(),
        "ticket should be null (no-ticket mode)"
    );
}

#[test]
fn clear_after_set_blanks_the_ticket_but_keeps_the_entry() {
    let env = Env::new();
    env.run(env.repo.path(), &["set-ticket", "POD-1"]).success();
    env.run(env.repo.path(), &["clear-ticket"]).success();
    let entry = env.state().get_branch("main").cloned().unwrap();
    assert!(entry.ticket.is_none());
}

#[test]
fn clear_ticket_outside_a_repo_errors() {
    let env = Env::new();
    let assert = env.run(env.home.path(), &["clear-ticket"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not in a git repo"));
}

#[test]
fn set_ticket_honors_repo_pattern_override() {
    let env = Env::new();
    std::fs::write(
        env.repo.path().join(".tix.toml"),
        "[ticket]\npattern = '^TIX-\\d+$'\n",
    )
    .unwrap();

    env.run(env.repo.path(), &["set-ticket", "POD-1"]).failure();
    env.run(env.repo.path(), &["set-ticket", "TIX-9"]).success();
    let entry = env.state().get_branch("main").cloned().unwrap();
    assert_eq!(entry.ticket.as_deref(), Some("TIX-9"));
}
