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

    fn git(&self, args: &[&str]) -> String {
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

    fn git_dir(&self) -> std::path::PathBuf {
        self.repo.path().join(".git")
    }

    fn show(&self, cwd: &Path) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .current_dir(cwd)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .arg("show")
            .assert()
    }

    fn write_state(&self, branch: &str, ticket: Option<&str>) {
        let mut state = State::empty();
        state.set_branch(
            branch,
            BranchEntry {
                ticket: ticket.map(str::to_string),
                set_at: Utc::now(),
                amended_through: None,
            },
        );
        state.save(&self.git_dir()).unwrap();
    }
}

fn stdout(a: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap()
}

#[test]
fn outside_a_repo_prints_branch_none_and_config_sources() {
    let env = Env::new();
    let out = env.show(env.home.path()).success();
    let s = stdout(&out);
    assert!(s.contains("Branch: (not in a git repo)"));
    assert!(!s.contains("Ticket:"));
    assert!(!s.contains("Protected branches"));
    assert!(s.contains("Config sources"));
    assert!(s.contains("default"));
}

#[test]
fn fresh_repo_with_no_state_says_ticket_not_set() {
    let env = Env::new();
    let out = env.show(env.repo.path()).success();
    let s = stdout(&out);
    assert!(s.contains("Branch: main"));
    assert!(s.contains("Ticket: (not set"));
    assert!(s.contains("Protected branches:"));
    assert!(s.contains("- main"));
    assert!(s.contains("Base: main"));
}

#[test]
fn protected_pattern_matching_current_branch_is_marked() {
    let env = Env::new();
    let out = env.show(env.repo.path()).success();
    let s = stdout(&out);
    let main_line = s
        .lines()
        .find(|l| l.trim_start().starts_with("- main"))
        .unwrap_or_else(|| panic!("no `- main` line in:\n{s}"));
    assert!(main_line.contains("← current"), "line: {main_line}");
}

#[test]
fn ticket_displays_when_set() {
    let env = Env::new();
    env.write_state("main", Some("POD-1234"));
    let out = env.show(env.repo.path()).success();
    let s = stdout(&out);
    assert!(s.contains("Ticket: POD-1234"), "stdout:\n{s}");
}

#[test]
fn no_ticket_mode_displays_distinctly_from_unset() {
    let env = Env::new();
    env.write_state("main", None);
    let out = env.show(env.repo.path()).success();
    let s = stdout(&out);
    assert!(s.contains("Ticket: (no-ticket mode)"), "stdout:\n{s}");
    assert!(!s.contains("not set"));
}

#[test]
fn repo_tix_toml_overrides_global_in_sources() {
    let env = Env::new();
    std::fs::write(
        env.repo.path().join(".tix.toml"),
        "[branches]\ndefault_base = \"trunk\"\n",
    )
    .unwrap();
    let out = env.show(env.repo.path()).success();
    let s = stdout(&out);
    assert!(s.contains("Base: trunk"), "stdout:\n{s}");
    let line = s
        .lines()
        .find(|l| l.contains("default base"))
        .unwrap_or_else(|| panic!("no default base line:\n{s}"));
    assert!(
        line.contains("repo"),
        "expected source=repo on line: {line}"
    );
}
