use std::path::Path;
use std::process::Command as ProcCommand;

use assert_cmd::Command;
use tempfile::{NamedTempFile, TempDir};

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
        env.git(&["commit", "--allow-empty", "-m", "initial"]);
        env
    }

    fn with_origin_url(&self, url: &str) {
        self.git(&["remote", "add", "origin", url]);
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

    fn force_url_mode(&self) {
        // Prevent tests from shelling out to a host-installed `gh` / `glab`.
        std::fs::write(
            self.repo.path().join(".tix.toml"),
            "[integrations]\npr_command = \"url\"\n",
        )
        .unwrap();
    }
}

fn stdout(a: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap()
}

#[test]
fn pr_prints_github_compare_url() {
    let env = Env::new();
    env.force_url_mode();
    env.with_origin_url(env.bare.path().to_str().unwrap());
    env.git(&["checkout", "-b", "feature/POD-1"]);
    env.git(&["commit", "--allow-empty", "-m", "POD-1 work"]);
    env.git(&["push", "-u", "origin", "feature/POD-1"]);
    env.git(&[
        "remote",
        "set-url",
        "origin",
        "git@github.com:owner/repo.git",
    ]);

    let assert = env.run_tix(env.repo.path(), &["pr"]).success();
    let out = stdout(&assert);
    assert!(
        out.contains("github.com/owner/repo/compare/feature/POD-1"),
        "stdout: {out}"
    );
    assert!(out.contains("expand=1"));
}

#[test]
fn pr_errors_when_no_upstream() {
    let env = Env::new();
    env.force_url_mode();
    env.with_origin_url("git@github.com:owner/repo.git");
    env.git(&["checkout", "-b", "feature/POD-1"]);
    let assert = env.run_tix(env.repo.path(), &["pr"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("git push -u origin feature/POD-1"));
}

#[test]
fn pr_outside_a_repo_errors() {
    let env = Env::new();
    let assert = env.run_tix(env.home.path(), &["pr"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not in a git repo"));
}
