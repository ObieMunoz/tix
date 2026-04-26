#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// Empty config file path used to neutralize the developer's real
/// `~/.gitconfig`. Without this, an installed `tix` on the dev machine
/// would inject its protected-branches hook into every test repo and
/// reject `git commit -m "initial"` on `main`. /dev/null parses as an
/// empty config on every Unix git we support.
const NULL_GLOBAL: &str = "/dev/null";

fn isolated_git(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd).env("GIT_CONFIG_GLOBAL", NULL_GLOBAL);
    cmd
}

pub fn run_git(cwd: &Path, args: &[&str]) -> String {
    let output = isolated_git(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawning git {args:?}: {e}"));
    if !output.status.success() {
        panic!(
            "git {args:?} failed in {}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub fn init_repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = canonical(dir.path());
    isolated_git(&path)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    run_git(&path, &["config", "user.email", "test@example.com"]);
    run_git(&path, &["config", "user.name", "Test"]);
    run_git(&path, &["config", "commit.gpgsign", "false"]);
    dir
}

pub fn init_bare() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    isolated_git(dir.path())
        .args(["init", "--bare", "-b", "main"])
        .output()
        .unwrap();
    dir
}

pub fn init_with_origin() -> (TempDir, TempDir) {
    let bare = init_bare();
    let work = init_repo();
    let bare_path = canonical(bare.path());
    run_git(
        &canonical(work.path()),
        &["remote", "add", "origin", bare_path.to_str().unwrap()],
    );
    (bare, work)
}

pub fn commit(cwd: &Path, msg: &str) -> String {
    run_git(cwd, &["commit", "--allow-empty", "-m", msg]);
    run_git(cwd, &["rev-parse", "HEAD"])
}

pub fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap()
}

/// Tests that exercise the `Git` wrapper directly should construct it
/// via this helper so the wrapper inherits the same `GIT_CONFIG_GLOBAL`
/// override — otherwise an installed `tix` on the developer's machine
/// would route the test commit through real hooks.
pub fn isolated_wrapper(path: &Path) -> tix_git::git::Git {
    tix_git::git::Git::at(path).with_env("GIT_CONFIG_GLOBAL", NULL_GLOBAL)
}
