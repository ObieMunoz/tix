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

    fn git_dir(&self) -> std::path::PathBuf {
        self.repo.path().join(".git")
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
        s.save(&self.git_dir()).unwrap();
    }

    fn run_hook(&self, cwd: &Path) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .current_dir(cwd)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(["hook", "pre-commit"])
            .assert()
    }
}

#[test]
fn no_op_when_branch_already_has_state_entry() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    env.run_hook(env.repo.path()).success();
}

#[test]
fn no_op_when_branch_is_in_no_ticket_mode() {
    let env = Env::new();
    env.write_state("main", None);
    env.run_hook(env.repo.path()).success();
}

#[test]
fn fails_clean_when_no_state_and_stdin_is_not_a_tty() {
    let env = Env::new();
    let assert = env.run_hook(env.repo.path()).failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("not a terminal"),
        "expected non-TTY hint: {stderr}"
    );
    assert!(
        stderr.contains("tix set-ticket") || stderr.contains("tix clear-ticket"),
        "expected set-ticket / clear-ticket hint: {stderr}"
    );
}

#[test]
fn end_to_end_git_commit_on_a_fresh_branch_without_state_blocks() {
    let env = Env::new();

    Command::cargo_bin("tix")
        .unwrap()
        .current_dir(env.repo.path())
        .env("HOME", env.home.path())
        .env("XDG_CONFIG_HOME", env.xdg.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .arg("init")
        .assert()
        .success();

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
        .args(["commit", "--allow-empty", "-m", "fix"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "git commit should be blocked when branch has no state in non-TTY context"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not a terminal") || stderr.contains("tix set-ticket"),
        "expected guidance toward set-ticket: {stderr}"
    );
}
