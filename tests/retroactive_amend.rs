use std::path::Path;
use std::process::Command as ProcCommand;

use assert_cmd::Command;
use tempfile::{NamedTempFile, TempDir};
use tix_git::state::State;

struct Env {
    home: TempDir,
    xdg: TempDir,
    git_global: NamedTempFile,
    bare: TempDir,
    repo: TempDir,
}

impl Env {
    fn new() -> Self {
        let bare = tempfile::tempdir().unwrap();
        ProcCommand::new("git")
            .current_dir(bare.path())
            .args(["init", "--bare", "-b", "main"])
            .output()
            .unwrap();

        let env = Self {
            home: tempfile::tempdir().unwrap(),
            xdg: tempfile::tempdir().unwrap(),
            git_global: NamedTempFile::new().unwrap(),
            bare,
            repo: tempfile::tempdir().unwrap(),
        };
        env.git(&["init", "-b", "main"]);
        env.git(&["config", "user.email", "test@example.com"]);
        env.git(&["config", "user.name", "Test"]);
        env.git(&["config", "commit.gpgsign", "false"]);
        env.git(&["remote", "add", "origin", env.bare.path().to_str().unwrap()]);
        env.git(&["commit", "--allow-empty", "-m", "initial"]);
        env.git(&["push", "origin", "main"]);
        env.git(&["fetch", "origin", "main"]);
        env
    }

    fn git(&self, args: &[&str]) -> String {
        let out = ProcCommand::new("git")
            .current_dir(self.repo.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(args)
            .output()
            .unwrap();
        if !out.status.success() {
            panic!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn empty_commit(&self, msg: &str) -> String {
        self.git(&["commit", "--allow-empty", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn run_tix(&self, cwd: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .current_dir(cwd)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(args)
            .assert()
    }

    fn subjects(&self) -> Vec<String> {
        self.git(&["log", "--format=%s"])
            .lines()
            .map(String::from)
            .collect()
    }

    fn state(&self) -> State {
        State::load(&self.repo.path().join(".git")).unwrap()
    }
}

#[test]
fn amend_rewrites_all_unprefixed_unpushed_commits() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    env.empty_commit("first commit");
    env.empty_commit("second commit");
    env.empty_commit("third commit");

    env.run_tix(env.repo.path(), &["set-ticket", "POD-1", "--yes"])
        .success();

    let subjects = env.subjects();
    assert_eq!(subjects[0], "POD-1 third commit");
    assert_eq!(subjects[1], "POD-1 second commit");
    assert_eq!(subjects[2], "POD-1 first commit");
}

#[test]
fn amend_leaves_already_prefixed_commits_alone() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    env.empty_commit("first commit");
    env.empty_commit("POD-9 already tagged");
    env.empty_commit("third commit");

    env.run_tix(env.repo.path(), &["set-ticket", "POD-1", "--yes"])
        .success();

    let subjects = env.subjects();
    assert_eq!(subjects[0], "POD-1 third commit");
    assert_eq!(subjects[1], "POD-9 already tagged");
    assert_eq!(subjects[2], "POD-1 first commit");
}

#[test]
fn amend_preserves_commit_message_body() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    let body = "Detailed explanation\n\nLine three of the body";
    env.git(&["commit", "--allow-empty", "-m", "fix bug", "-m", body]);

    env.run_tix(env.repo.path(), &["set-ticket", "POD-1", "--yes"])
        .success();

    let full = env.git(&["log", "-1", "--format=%B"]);
    assert!(full.starts_with("POD-1 fix bug\n"));
    assert!(full.contains("Detailed explanation"));
    assert!(full.contains("Line three of the body"));
}

#[test]
fn amend_records_new_head_as_amended_through() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    env.empty_commit("commit one");
    env.empty_commit("commit two");

    env.run_tix(env.repo.path(), &["set-ticket", "POD-1", "--yes"])
        .success();

    let new_head = env.git(&["rev-parse", "HEAD"]);
    let entry = env.state().get_branch("feature").cloned().unwrap();
    assert_eq!(entry.amended_through.as_deref(), Some(new_head.as_str()));
}

#[test]
fn amend_refuses_when_a_candidate_is_on_remote_without_force() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    env.empty_commit("first commit");
    env.empty_commit("second commit");
    env.git(&["push", "origin", "feature"]);
    env.git(&["fetch", "origin", "feature"]);

    let assert = env
        .run_tix(env.repo.path(), &["set-ticket", "POD-1", "--yes"])
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("--force"), "stderr: {stderr}");
    let subjects = env.subjects();
    assert_eq!(subjects[0], "second commit", "no rewrite occurred");
}

#[test]
fn amend_force_yes_rewrites_even_when_on_remote() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    env.empty_commit("first commit");
    env.empty_commit("second commit");
    env.git(&["push", "origin", "feature"]);
    env.git(&["fetch", "origin", "feature"]);

    env.run_tix(
        env.repo.path(),
        &["set-ticket", "POD-1", "--force", "--yes"],
    )
    .success();
    let subjects = env.subjects();
    assert_eq!(subjects[0], "POD-1 second commit");
    assert_eq!(subjects[1], "POD-1 first commit");
}

#[test]
fn amend_no_op_when_nothing_to_rewrite() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    env.empty_commit("POD-9 already prefixed");

    env.run_tix(env.repo.path(), &["set-ticket", "POD-1", "--yes"])
        .success();

    let subjects = env.subjects();
    assert_eq!(subjects[0], "POD-9 already prefixed");
}

#[test]
fn amend_skipped_when_origin_base_missing() {
    let env = Env::new();
    env.git(&["checkout", "-b", "feature"]);
    env.empty_commit("only commit");
    env.git(&["remote", "remove", "origin"]);

    let assert = env
        .run_tix(env.repo.path(), &["set-ticket", "POD-1", "--yes"])
        .success();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("skipped retroactive amend"),
        "stderr: {stderr}"
    );
    let subjects = env.subjects();
    assert_eq!(subjects[0], "only commit");
}
