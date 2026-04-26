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
        env.git_in_repo(&["init", "-b", "main"]);
        env.git_in_repo(&["config", "user.email", "test@example.com"]);
        env.git_in_repo(&["config", "user.name", "Test"]);
        env.git_in_repo(&["config", "commit.gpgsign", "false"]);
        env.git_in_repo(&["commit", "--allow-empty", "-m", "initial"]);
        env
    }

    fn git_in_repo(&self, args: &[&str]) -> String {
        let out = ProcCommand::new("git")
            .current_dir(self.repo.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(args)
            .output()
            .unwrap();
        if !out.status.success() {
            panic!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn run_init(&self) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .arg("init")
            .assert()
    }

    fn run_doctor(&self, args: &[&str], cwd: &Path) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .current_dir(cwd)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .arg("doctor")
            .args(args)
            .assert()
    }

    fn hooks_dir(&self) -> std::path::PathBuf {
        self.xdg.path().join("tix").join("hooks")
    }

    fn config_file(&self) -> std::path::PathBuf {
        self.xdg.path().join("tix").join("config.toml")
    }
}

fn stdout(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

#[test]
fn healthy_install_inside_repo_exits_zero() {
    let env = Env::new();
    env.run_init().success();
    let out = env.run_doctor(&[], env.repo.path()).success();
    let s = stdout(&out);
    assert!(s.contains("git version"));
    assert!(s.contains("core.hooksPath"));
    assert!(s.contains("hook shims"));
    assert!(s.contains("global config"));
    assert!(s.contains("repo config"));
    assert!(!s.contains("FAIL"), "no FAIL on healthy install:\n{s}");
}

#[test]
fn doctor_outside_a_repo_skips_repo_checks() {
    let env = Env::new();
    env.run_init().success();
    let out = env.run_doctor(&[], env.home.path()).success();
    let s = stdout(&out);
    assert!(s.contains("global config"));
    assert!(
        !s.contains("repo config"),
        "should skip outside a repo:\n{s}"
    );
    assert!(!s.contains("default_base"));
}

#[test]
fn fail_when_a_shim_is_missing() {
    let env = Env::new();
    env.run_init().success();
    std::fs::remove_file(env.hooks_dir().join("pre-push")).unwrap();
    let out = env.run_doctor(&[], env.repo.path()).failure();
    let s = stdout(&out);
    assert!(s.contains("FAIL"));
    assert!(
        s.contains("pre-push"),
        "stdout should name the missing hook:\n{s}"
    );
}

#[test]
fn fail_when_a_shim_has_been_modified() {
    let env = Env::new();
    env.run_init().success();
    std::fs::write(
        env.hooks_dir().join("prepare-commit-msg"),
        "#!/bin/sh\necho hijacked\n",
    )
    .unwrap();
    let out = env.run_doctor(&[], env.repo.path()).failure();
    assert!(stdout(&out).contains("modified"));
}

#[test]
fn fail_when_global_config_is_unparseable() {
    let env = Env::new();
    env.run_init().success();
    std::fs::write(env.config_file(), "not valid TOML [[[").unwrap();
    let out = env.run_doctor(&[], env.repo.path()).failure();
    let s = stdout(&out);
    assert!(s.contains("global config"));
    assert!(s.contains("FAIL"));
}

#[test]
fn fail_when_hookspath_points_elsewhere() {
    let env = Env::new();
    env.run_init().success();
    ProcCommand::new("git")
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .args(["config", "--global", "core.hooksPath", "/tmp/elsewhere"])
        .output()
        .unwrap();
    let out = env.run_doctor(&[], env.repo.path()).failure();
    let s = stdout(&out);
    assert!(s.contains("/tmp/elsewhere"));
    assert!(s.contains("tix init --force"));
}

#[test]
fn warn_when_default_base_does_not_resolve() {
    let env = Env::new();
    env.run_init().success();
    let out = env.run_doctor(&[], env.repo.path()).success();
    let s = stdout(&out);
    assert!(s.contains("default_base"));
    assert!(
        s.contains("WARN"),
        "expected WARN for missing origin/main:\n{s}"
    );
}

#[test]
fn fail_when_repo_config_is_unparseable() {
    let env = Env::new();
    env.run_init().success();
    std::fs::write(env.repo.path().join(".tix.toml"), "broken [[[ toml").unwrap();
    let out = env.run_doctor(&[], env.repo.path()).failure();
    let s = stdout(&out);
    assert!(s.contains("repo config"));
    assert!(s.contains("FAIL"));
}

#[test]
fn verbose_flag_adds_detail_for_failures() {
    let env = Env::new();
    env.run_init().success();
    std::fs::write(env.config_file(), "not valid TOML [[[").unwrap();
    let normal = env.run_doctor(&[], env.repo.path()).failure();
    let verbose = env.run_doctor(&["--verbose"], env.repo.path()).failure();
    assert!(stdout(&verbose).len() > stdout(&normal).len());
}

#[test]
fn doctor_warns_about_signing_key_when_gpgsign_enabled() {
    let env = Env::new();
    env.run_init().success();
    ProcCommand::new("git")
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .args(["config", "--global", "commit.gpgsign", "true"])
        .output()
        .unwrap();
    let out = env.run_doctor(&[], env.repo.path()).success();
    let s = stdout(&out);
    assert!(s.contains("signing key"));
    assert!(s.contains("WARN"));
}
