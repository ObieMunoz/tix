use std::path::Path;
use std::process::Command as ProcCommand;

use assert_cmd::Command;
use tempfile::{NamedTempFile, TempDir};

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

    fn global_file(&self) -> std::path::PathBuf {
        self.xdg.path().join("tix").join("config.toml")
    }

    fn repo_file(&self) -> std::path::PathBuf {
        self.repo.path().join(".tix.toml")
    }
}

fn stdout(a: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap()
}

#[test]
fn protect_appends_to_global_protected_list() {
    let env = Env::new();
    let out = env.run(env.repo.path(), &["protect", "trunk"]).success();
    let body = std::fs::read_to_string(env.global_file()).unwrap();
    assert!(body.contains("\"trunk\""), "expected trunk in {body}");
    let s = stdout(&out);
    assert!(s.contains("trunk"));
    assert!(
        s.contains("main"),
        "default-protected entries should still be visible"
    );
}

#[test]
fn protect_is_idempotent() {
    let env = Env::new();
    env.run(env.repo.path(), &["protect", "trunk"]).success();
    env.run(env.repo.path(), &["protect", "trunk"]).success();
    let body = std::fs::read_to_string(env.global_file()).unwrap();
    assert_eq!(body.matches("\"trunk\"").count(), 1);
}

#[test]
fn protect_repo_writes_dot_tix_toml() {
    let env = Env::new();
    env.run(env.repo.path(), &["protect", "trunk", "--repo"])
        .success();
    let body = std::fs::read_to_string(env.repo_file()).unwrap();
    assert!(body.contains("\"trunk\""));
}

#[test]
fn protect_repo_outside_a_repo_errors() {
    let env = Env::new();
    let assert = env
        .run(env.home.path(), &["protect", "trunk", "--repo"])
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("git repo"));
}

#[test]
fn unprotect_removes_a_pattern_globally() {
    let env = Env::new();
    env.run(env.repo.path(), &["protect", "trunk"]).success();
    env.run(env.repo.path(), &["unprotect", "trunk"]).success();
    let body = std::fs::read_to_string(env.global_file()).unwrap();
    assert!(!body.contains("\"trunk\""));
}

#[test]
fn unprotect_absent_pattern_emits_warning_but_succeeds() {
    let env = Env::new();
    let assert = env
        .run(env.repo.path(), &["unprotect", "never-added"])
        .success();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("warning") && stderr.contains("never-added"),
        "stderr: {stderr}"
    );
}

#[test]
fn protect_glob_pattern_persists_unchanged() {
    let env = Env::new();
    env.run(env.repo.path(), &["protect", "release/*"])
        .success();
    let body = std::fs::read_to_string(env.global_file()).unwrap();
    assert!(body.contains("release/*"));
}
