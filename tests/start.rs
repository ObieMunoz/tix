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
        // Move off `main` so `tix start` doesn't refuse based on dirty
        // tree from a future test that staged a file on main; not strictly
        // necessary but matches typical user state.
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

    fn state(&self) -> State {
        State::load(&self.repo.path().join(".git")).unwrap()
    }
}

fn stdout(a: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap()
}

#[test]
fn start_creates_branch_and_registers_ticket() {
    let env = Env::new();
    let assert = env.run_tix(env.repo.path(), &["start", "POD-1"]).success();
    assert_eq!(
        env.git(&["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature/POD-1"
    );
    let entry = env.state().get_branch("feature/POD-1").cloned().unwrap();
    assert_eq!(entry.ticket.as_deref(), Some("POD-1"));
    assert!(stdout(&assert).contains("Started feature/POD-1 off main"));
}

#[test]
fn start_with_description_slugifies_into_branch_name() {
    let env = Env::new();
    env.run_tix(env.repo.path(), &["start", "POD-1", "Fix Login Bug"])
        .success();
    assert_eq!(
        env.git(&["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature/POD-1-fix-login-bug"
    );
}

#[test]
fn start_with_base_flag_uses_that_remote_ref() {
    let env = Env::new();
    // Create a develop branch on the remote so origin/develop exists.
    env.git(&["branch", "develop"]);
    env.git(&["push", "origin", "develop"]);
    env.git(&["fetch", "origin", "develop"]);

    env.run_tix(env.repo.path(), &["start", "POD-2", "--base", "develop"])
        .success();
    assert_eq!(
        env.git(&["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature/POD-2"
    );
}

#[test]
fn start_refuses_when_working_tree_is_dirty() {
    let env = Env::new();
    std::fs::write(env.repo.path().join("dirty.txt"), "x").unwrap();
    let assert = env.run_tix(env.repo.path(), &["start", "POD-1"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("dirty"), "stderr: {stderr}");
}

#[test]
fn start_refuses_when_branch_already_exists() {
    let env = Env::new();
    env.run_tix(env.repo.path(), &["start", "POD-1"]).success();
    // Switch back to main so we can re-run start
    env.git(&["checkout", "main"]);

    let assert = env.run_tix(env.repo.path(), &["start", "POD-1"]).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("already exists"), "stderr: {stderr}");
}

#[test]
fn start_rejects_invalid_ticket() {
    let env = Env::new();
    let assert = env
        .run_tix(env.repo.path(), &["start", "not-a-ticket"])
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not-a-ticket"));
}

#[test]
fn start_fails_when_origin_base_does_not_exist() {
    let env = Env::new();
    let assert = env
        .run_tix(env.repo.path(), &["start", "POD-1", "--base", "ghost"])
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("ghost") || stderr.contains("fetching"),
        "stderr: {stderr}"
    );
}

#[test]
fn start_honors_repo_start_prefix_override() {
    let env = Env::new();
    std::fs::write(
        env.repo.path().join(".tix.toml"),
        "[branches]\nstart_prefix = \"bug\"\n",
    )
    .unwrap();
    env.git(&["add", ".tix.toml"]);
    env.git(&["commit", "-m", "POD-99 add tix config"]);
    env.git(&["push", "origin", "main"]);
    env.git(&["fetch", "origin", "main"]);

    env.run_tix(env.repo.path(), &["start", "POD-1"]).success();
    assert_eq!(env.git(&["rev-parse", "--abbrev-ref", "HEAD"]), "bug/POD-1");
}
