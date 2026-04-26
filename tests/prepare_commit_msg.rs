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

    fn run_hook(&self, msg_path: &Path, source: &str) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .current_dir(self.repo.path())
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args([
                "hook",
                "prepare-commit-msg",
                msg_path.to_str().unwrap(),
                source,
            ])
            .assert()
    }

    fn write_msg(&self, body: &str) -> std::path::PathBuf {
        let p = self.repo.path().join("COMMIT_EDITMSG");
        std::fs::write(&p, body).unwrap();
        p
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn prefixes_a_simple_message() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg("fix bug\n");
    env.run_hook(&msg, "message").success();
    assert_eq!(read(&msg), "POD-1 fix bug\n");
}

#[test]
fn idempotent_when_same_ticket_already_present() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg("POD-1 fix bug\n");
    env.run_hook(&msg, "message").success();
    assert_eq!(read(&msg), "POD-1 fix bug\n");
}

#[test]
fn leaves_a_different_ticket_alone() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg("POD-9 already tagged\n");
    env.run_hook(&msg, "message").success();
    assert_eq!(read(&msg), "POD-9 already tagged\n");
}

#[test]
fn skips_merge_commits() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg("Merge branch 'feature'\n");
    env.run_hook(&msg, "merge").success();
    assert_eq!(read(&msg), "Merge branch 'feature'\n");
}

#[test]
fn skips_squash_commits() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg("Squashed commit\n");
    env.run_hook(&msg, "squash").success();
    assert_eq!(read(&msg), "Squashed commit\n");
}

#[test]
fn no_state_entry_is_a_no_op() {
    let env = Env::new();
    let msg = env.write_msg("fix bug\n");
    env.run_hook(&msg, "message").success();
    assert_eq!(read(&msg), "fix bug\n");
}

#[test]
fn no_ticket_mode_is_a_no_op() {
    let env = Env::new();
    env.write_state("main", None);
    let msg = env.write_msg("fix bug\n");
    env.run_hook(&msg, "message").success();
    assert_eq!(read(&msg), "fix bug\n");
}

#[test]
fn applies_to_amend_source() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg("amended message\n");
    env.run_hook(&msg, "commit").success();
    assert_eq!(read(&msg), "POD-1 amended message\n");
}

#[test]
fn applies_to_template_with_leading_comments() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg(
        "\n# Please enter the commit message\n# Lines starting with '#' will be ignored\n",
    );
    env.run_hook(&msg, "message").success();
    let result = read(&msg);
    assert!(
        result.starts_with("POD-1 \n"),
        "expected prefix on a new first line, got: {result:?}"
    );
}

#[test]
fn end_to_end_git_commit_prefixes_subject_via_real_hook() {
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
    Command::cargo_bin("tix")
        .unwrap()
        .current_dir(env.repo.path())
        .env("HOME", env.home.path())
        .env("XDG_CONFIG_HOME", env.xdg.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .args(["set-ticket", "POD-1"])
        .assert()
        .success();

    // Put the cargo-built tix on PATH so the shim can find it.
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
        .args(["commit", "--allow-empty", "-m", "fix bug"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let subj = ProcCommand::new("git")
        .current_dir(env.repo.path())
        .env("GIT_CONFIG_GLOBAL", env.git_global.path())
        .args(["log", "-1", "--format=%s"])
        .output()
        .unwrap();
    let subject = String::from_utf8_lossy(&subj.stdout).trim().to_string();
    assert_eq!(subject, "POD-1 fix bug");
}

#[test]
fn body_text_is_preserved_when_only_subject_is_prefixed() {
    let env = Env::new();
    env.write_state("main", Some("POD-1"));
    let msg = env.write_msg("fix bug\n\nlong description\nspans multiple lines\n");
    env.run_hook(&msg, "message").success();
    assert_eq!(
        read(&msg),
        "POD-1 fix bug\n\nlong description\nspans multiple lines\n"
    );
}
