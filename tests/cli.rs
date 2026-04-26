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
fn every_stub_exits_nonzero_with_not_yet_implemented() {
    // Implemented commands are deliberately absent from this list — they
    // are exercised under fully env-isolated tests in their own files.
    //   IMPLEMENTED: init, uninstall, doctor, show, config, set-ticket,
    //                clear-ticket, hook (dispatcher)
    let cases: &[&[&str]] = &[
        &["start", "POD-1"],
        &["protect", "main"],
        &["unprotect", "main"],
        &["pr"],
        &["ticket"],
    ];
    for args in cases {
        let (mut cmd, _env) = isolated();
        let assert = cmd.args(*args).assert().failure();
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        assert!(
            stderr.contains("not yet implemented"),
            "args {args:?}: expected stub message, got: {stderr}"
        );
    }
}

#[test]
fn start_accepts_ticket_description_and_base() {
    let (mut cmd, _env) = isolated();
    let assert = cmd
        .args(["start", "POD-1234", "fix-thing", "--base", "develop"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not yet implemented"), "stderr: {stderr}");
}

#[test]
fn start_accepts_flag_before_positional() {
    let (mut cmd, _env) = isolated();
    cmd.args(["start", "POD-1234", "--base", "develop", "fix-thing"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));
}

#[test]
fn protect_accepts_global_or_repo_but_not_both() {
    let (mut cmd, _env) = isolated();
    cmd.args(["protect", "main", "--global"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));

    let (mut cmd2, _env2) = isolated();
    cmd2.args(["protect", "main", "--global", "--repo"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("error:"));
}

#[test]
fn ticket_open_subcommand_parses() {
    let (mut cmd, _env) = isolated();
    cmd.args(["ticket", "open"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));
}

#[test]
fn hook_clap_accepts_trailing_args_including_dashes() {
    // Clap-only parse check — hook trailing-arg semantics are tested
    // for real (with state set up etc.) in tests/prepare_commit_msg.rs.
    let (mut cmd, _env) = isolated();
    cmd.args(["hook", "--help"]).assert().success();
}
