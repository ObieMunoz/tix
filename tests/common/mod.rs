#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

pub fn run_git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
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
    Command::new("git")
        .current_dir(&path)
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
    Command::new("git")
        .current_dir(dir.path())
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
