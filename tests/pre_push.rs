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
        env.git(&["remote", "add", "origin", env.bare.path().to_str().unwrap()]);
        env.git(&["commit", "--allow-empty", "-m", "initial"]);
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

    fn run_tix(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .current_dir(self.repo.path())
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(args)
            .assert()
    }

    fn git_push(&self, refspec: &str) -> std::process::Output {
        let tix_path = assert_cmd::cargo::cargo_bin("tix");
        let tix_dir = tix_path.parent().unwrap();
        let path = format!(
            "{}:{}",
            tix_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        ProcCommand::new("git")
            .current_dir(self.repo.path())
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .env("PATH", &path)
            .args(["push", "origin", refspec])
            .output()
            .unwrap()
    }
}

#[test]
fn end_to_end_push_to_protected_branch_blocked() {
    let env = Env::new();
    env.run_tix(&["init"]).success();
    let out = env.git_push("main");
    assert!(
        !out.status.success(),
        "push to main should be blocked by pre-push hook"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("protected") || stderr.contains("push blocked"),
        "expected protection message: {stderr}"
    );
}

#[test]
fn end_to_end_push_unprotected_branch_succeeds() {
    let env = Env::new();
    env.run_tix(&["init"]).success();
    env.git(&["checkout", "-b", "feature/test"]);
    env.git(&["commit", "--allow-empty", "-m", "POD-1 work"]);
    let out = env.git_push("feature/test");
    assert!(
        out.status.success(),
        "push to feature should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn end_to_end_branch_deletion_allowed_on_protected() {
    let env = Env::new();
    // First push a branch, then delete it via push.
    env.git(&["checkout", "-b", "release/old"]);
    env.git(&["commit", "--allow-empty", "-m", "POD-1 work"]);
    let push_out = env.git_push("release/old");
    assert!(
        push_out.status.success(),
        "initial push: {}",
        String::from_utf8_lossy(&push_out.stderr)
    );
    env.git(&["checkout", "main"]);

    // Now install tix protection and try to delete the protected ref.
    env.run_tix(&["init"]).success();
    let tix_path = assert_cmd::cargo::cargo_bin("tix");
    let tix_dir = tix_path.parent().unwrap();
    let path = format!(
        "{}:{}",
        tix_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = ProcCommand::new("git")
        .current_dir(env.repo.path())
        .env("HOME", env.home.path())
        .env("XDG_CONFIG_HOME", env.xdg.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .env("PATH", &path)
        .args(["push", "origin", "--delete", "release/old"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "deletion of a protected ref should be allowed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn end_to_end_push_no_verify_bypasses_pre_push() {
    let env = Env::new();
    env.run_tix(&["init"]).success();
    let tix_path = assert_cmd::cargo::cargo_bin("tix");
    let tix_dir = tix_path.parent().unwrap();
    let path = format!(
        "{}:{}",
        tix_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = ProcCommand::new("git")
        .current_dir(env.repo.path())
        .env("HOME", env.home.path())
        .env("XDG_CONFIG_HOME", env.xdg.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .env("PATH", &path)
        .args(["push", "--no-verify", "origin", "main"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "--no-verify must bypass pre-push: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn naming_block_mode_blocks_push_of_nonconforming_branch() {
    let env = Env::new();
    env.run_tix(&["init"]).success();
    std::fs::write(
        env.repo.path().join(".tix.toml"),
        "[branches]\nnaming_enforcement = \"block\"\n",
    )
    .unwrap();
    env.git(&["checkout", "-b", "wip"]);
    env.git(&["commit", "--allow-empty", "-m", "POD-1 work"]);
    let out = env.git_push("wip");
    assert!(!out.status.success(), "push of wip should be blocked");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("naming pattern"), "stderr: {stderr}");
}

#[test]
fn stale_base_warning_fires_when_branch_is_far_behind() {
    let env = Env::new();
    env.run_tix(&["init"]).success();
    // push initial main to bare
    let push_main = env.git_push("main");
    // The push of main is blocked by protection; instead set up via direct push.
    if !push_main.status.success() {
        // Push using --no-verify to populate the bare repo
        let tix_path = assert_cmd::cargo::cargo_bin("tix");
        let tix_dir = tix_path.parent().unwrap();
        let path = format!(
            "{}:{}",
            tix_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let _ = ProcCommand::new("git")
            .current_dir(env.repo.path())
            .env("HOME", env.home.path())
            .env("XDG_CONFIG_HOME", env.xdg.path())
            .env("GIT_CONFIG_GLOBAL", env.git_global.path())
            .env("PATH", &path)
            .args(["push", "--no-verify", "origin", "main"])
            .output();
    }

    env.git(&["checkout", "-b", "feature/test"]);
    env.git(&["commit", "--allow-empty", "-m", "POD-1 work"]);

    // Lower the threshold to 0 commits behind so it always triggers
    // (we only need to verify the wiring); but threshold = 0 disables.
    // Set threshold to 1, then add 2 commits to main on the remote.
    std::fs::write(
        env.repo.path().join(".tix.toml"),
        "[push]\nstale_warn_threshold = 1\n",
    )
    .unwrap();
    env.git(&["add", ".tix.toml"]);
    env.git(&["commit", "-m", "POD-1 add config"]);

    // Push some new main commits via a sibling clone.
    let sibling = tempfile::tempdir().unwrap();
    ProcCommand::new("git")
        .current_dir(sibling.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .args(["clone", env.bare.path().to_str().unwrap(), "."])
        .output()
        .unwrap();
    for i in 0..3 {
        ProcCommand::new("git")
            .current_dir(sibling.path())
            .env("GIT_CONFIG_GLOBAL", env.git_global.path())
            .args([
                "commit",
                "--allow-empty",
                "-m",
                &format!("POD-2 main commit {i}"),
            ])
            .output()
            .unwrap();
    }
    ProcCommand::new("git")
        .current_dir(sibling.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .args(["push", "origin", "main"])
        .output()
        .unwrap();

    let out = env.git_push("feature/test");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("commits behind") || stderr.contains("rebase"),
        "expected stale-base warning: {stderr}"
    );
}

#[test]
fn nonexistent_repo_root_does_not_crash_pre_push_hook() {
    // Run pre-push hook outside any git repo (no remote refs/branches),
    // confirm it exits 0 cleanly.
    let env = Env::new();
    Command::cargo_bin("tix")
        .unwrap()
        .current_dir(Path::new("/tmp"))
        .env("HOME", env.home.path())
        .env("XDG_CONFIG_HOME", env.xdg.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .args(["hook", "pre-push", "origin", "ssh://example/repo"])
        .write_stdin("")
        .assert()
        .success();
}
