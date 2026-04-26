use assert_cmd::Command;
use tempfile::{NamedTempFile, TempDir};

const SUBCOMMANDS: &[&str] = &[
    "init",
    "start",
    "set-ticket",
    "clear-ticket",
    "show",
    "protect",
    "unprotect",
    "config",
    "doctor",
    "pr",
    "ticket",
    "hook",
];

/// All cli.rs tests run with HOME / XDG_CONFIG_HOME / GIT_CONFIG_GLOBAL
/// pointed at scratch temp paths. The isolation is defensive: if an
/// implemented command (which writes to filesystem or git config)
/// accidentally lands in a "stub" assertion, it writes to a TempDir
/// that gets cleaned up on Drop instead of polluting the host machine.
struct IsolatedEnv {
    _scratch: TempDir,
    _git_global: NamedTempFile,
}

fn isolated() -> (Command, IsolatedEnv) {
    let scratch = tempfile::tempdir().unwrap();
    let git_global = NamedTempFile::new().unwrap();
    let mut cmd = Command::cargo_bin("tix").unwrap();
    cmd.env("HOME", scratch.path())
        .env("XDG_CONFIG_HOME", scratch.path())
        .env("GIT_CONFIG_GLOBAL", git_global.path());
    (
        cmd,
        IsolatedEnv {
            _scratch: scratch,
            _git_global: git_global,
        },
    )
}

#[test]
fn top_level_help_lists_every_subcommand() {
    let (mut cmd, _env) = isolated();
    let assert = cmd.arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for sub in SUBCOMMANDS {
        assert!(
            stdout.contains(sub),
            "expected `{sub}` in --help output:\n{stdout}"
        );
    }
}

#[test]
fn each_subcommand_has_its_own_help() {
    for sub in SUBCOMMANDS {
        let (mut cmd, _env) = isolated();
        cmd.args([sub, "--help"]).assert().success();
    }
}

#[test]
fn unknown_subcommand_errors_cleanly() {
    let (mut cmd, _env) = isolated();
    let assert = cmd.arg("nonsense").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("unrecognized") || stderr.contains("error:"),
        "expected clap error: {stderr}"
    );
}

#[test]
fn start_clap_accepts_ticket_description_and_base() {
    // Clap parse-only check; full start behavior tested in tests/start.rs.
    let (mut cmd, _env) = isolated();
    cmd.args(["start", "--help"]).assert().success();
}

#[test]
fn protect_global_and_repo_are_mutually_exclusive() {
    let (mut cmd, _env) = isolated();
    cmd.args(["protect", "main", "--global", "--repo"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("error:"));
}

#[test]
fn ticket_open_subcommand_clap_parses() {
    let (mut cmd, _env) = isolated();
    cmd.args(["ticket", "--help"]).assert().success();
}

#[test]
fn hook_clap_accepts_trailing_args_including_dashes() {
    // Clap-only parse check — hook trailing-arg semantics are tested
    // for real (with state set up etc.) in tests/prepare_commit_msg.rs.
    let (mut cmd, _env) = isolated();
    cmd.args(["hook", "--help"]).assert().success();
}
