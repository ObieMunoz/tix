use std::path::Path;
use std::process::Command as ProcCommand;

use assert_cmd::Command;
use chrono::Utc;
use tempfile::{NamedTempFile, TempDir};
use tix_git::state::{BranchEntry, State};

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

    fn write_template(&self, template: &str) {
        self.run(
            self.repo.path(),
            &[
                "config",
                "set",
                "integrations.ticket_url_template",
                template,
            ],
        )
        .success();
    }

    fn write_state(&self, branch: &str, ticket: Option<&str>) {
        let mut s = State::empty();
        s.set_branch(
            branch,
            BranchEntry {
                ticket: ticket.map(str::to_string),
                set_at: Utc::now(),
                amended_through: None,
            },
        );
        s.save(&self.repo.path().join(".git")).unwrap();
    }
}

fn stdout(a: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap()
}

#[test]
fn ticket_prints_url_with_substituted_ticket() {
    let env = Env::new();
    env.write_template("https://example.atlassian.net/browse/{ticket}");
    env.write_state("main", Some("POD-1234"));
    let assert = env.run(env.repo.path(), &["ticket"]).success();
    let out = stdout(&assert);
    assert!(out.contains("https://example.atlassian.net/browse/POD-1234"));
}

#[test]
fn ticket_errors_when_template_is_empty() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let assert = env.run(env.repo.path(), &["ticket"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("ticket_url_template"));
}

#[test]
fn ticket_errors_when_branch_has_no_ticket_set() {
    let env = Env::new();
    env.write_template("https://example.atlassian.net/browse/{ticket}");
    let assert = env.run(env.repo.path(), &["ticket"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("no ticket"));
    assert!(stderr.contains("set-ticket"));
}

#[test]
fn ticket_errors_in_no_ticket_mode() {
    let env = Env::new();
    env.write_template("https://example.atlassian.net/browse/{ticket}");
    env.write_state("main", None);
    let assert = env.run(env.repo.path(), &["ticket"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("no-ticket mode"));
}

#[test]
fn ticket_outside_a_repo_errors() {
    let env = Env::new();
    let assert = env.run(env.home.path(), &["ticket"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not in a git repo"));
}
