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
fn get_default_when_nothing_configured() {
    let env = Env::new();
    let out = env
        .run(env.repo.path(), &["config", "get", "branches.default_base"])
        .success();
    let s = stdout(&out);
    assert!(s.contains("branches.default_base"));
    assert!(s.contains("\"main\""));
    assert!(s.contains("(default)"));
}

#[test]
fn get_unknown_key_lists_known_keys() {
    let env = Env::new();
    let assert = env
        .run(env.repo.path(), &["config", "get", "no.such.key"])
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("unknown key"));
    assert!(stderr.contains("branches.default_base"));
}

#[test]
fn set_global_string_then_get_reflects_it() {
    let env = Env::new();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.default_base", "develop"],
    )
    .success();
    assert!(env.global_file().exists());
    let out = env
        .run(env.repo.path(), &["config", "get", "branches.default_base"])
        .success();
    let s = stdout(&out);
    assert!(s.contains("\"develop\""));
    assert!(s.contains("(global)"));
}

#[test]
fn set_repo_writes_dot_tix_toml() {
    let env = Env::new();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.default_base", "trunk", "--repo"],
    )
    .success();
    assert!(env.repo_file().exists());
    let body = std::fs::read_to_string(env.repo_file()).unwrap();
    assert!(body.contains("default_base"));
    assert!(body.contains("trunk"));
}

#[test]
fn set_repo_outside_a_repo_errors() {
    let env = Env::new();
    let assert = env
        .run(
            env.home.path(),
            &["config", "set", "branches.default_base", "trunk", "--repo"],
        )
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("git repo"));
}

#[test]
fn set_integer_type_error_is_clear() {
    let env = Env::new();
    let assert = env
        .run(
            env.repo.path(),
            &["config", "set", "push.stale_warn_threshold", "abc"],
        )
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("expected integer"));
    assert!(!env.global_file().exists(), "must not write on type error");
}

#[test]
fn set_list_with_scalar_value_suggests_append() {
    let env = Env::new();
    let assert = env
        .run(
            env.repo.path(),
            &["config", "set", "branches.protected", "release/*"],
        )
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("--append"));
}

#[test]
fn append_adds_to_protected_list_and_is_idempotent() {
    let env = Env::new();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.protected", "--append", "trunk"],
    )
    .success();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.protected", "--append", "trunk"],
    )
    .success();
    let body = std::fs::read_to_string(env.global_file()).unwrap();
    let count = body.matches("\"trunk\"").count();
    assert_eq!(count, 1, "append must be idempotent: {body}");
}

#[test]
fn remove_drops_an_entry_from_protected() {
    let env = Env::new();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.protected", "--append", "trunk"],
    )
    .success();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.protected", "--remove", "trunk"],
    )
    .success();
    let body = std::fs::read_to_string(env.global_file()).unwrap();
    assert!(!body.contains("\"trunk\""), "trunk should be gone: {body}");
}

#[test]
fn list_default_shows_all_keys_with_sources() {
    let env = Env::new();
    let out = env.run(env.repo.path(), &["config", "list"]).success();
    let s = stdout(&out);
    for key in [
        "ticket.pattern",
        "branches.default_base",
        "push.stale_warn_threshold",
        "integrations.pr_provider",
    ] {
        assert!(s.contains(key), "missing {key}:\n{s}");
    }
    assert!(s.contains("(default)"));
}

#[test]
fn list_global_shows_only_globally_set_keys() {
    let env = Env::new();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.default_base", "develop"],
    )
    .success();
    let out = env
        .run(env.repo.path(), &["config", "list", "--global"])
        .success();
    let s = stdout(&out);
    assert!(s.contains("branches.default_base"));
    assert!(
        !s.contains("ticket.pattern"),
        "global list should only show keys present in the file:\n{s}"
    );
}

#[test]
fn list_repo_shows_only_repo_set_keys() {
    let env = Env::new();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.default_base", "trunk", "--repo"],
    )
    .success();
    let out = env
        .run(env.repo.path(), &["config", "list", "--repo"])
        .success();
    let s = stdout(&out);
    assert!(s.contains("branches.default_base"));
    assert!(s.contains("trunk"));
}

#[test]
fn round_trip_load_after_set_does_not_corrupt_file() {
    let env = Env::new();
    env.run(
        env.repo.path(),
        &["config", "set", "branches.default_base", "develop"],
    )
    .success();
    env.run(
        env.repo.path(),
        &["config", "set", "ticket.pattern", "^[A-Z]+-\\d+$"],
    )
    .success();
    env.run(env.repo.path(), &["config", "list", "--all"])
        .success();
    env.run(env.repo.path(), &["show"]).success();
}
