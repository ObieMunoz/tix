use assert_cmd::Command;

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

#[test]
fn top_level_help_lists_every_subcommand() {
    let assert = Command::cargo_bin("tix")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
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
        Command::cargo_bin("tix")
            .unwrap()
            .args([sub, "--help"])
            .assert()
            .success();
    }
}

#[test]
fn unknown_subcommand_errors_cleanly() {
    let assert = Command::cargo_bin("tix")
        .unwrap()
        .arg("nonsense")
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("unrecognized") || stderr.contains("error:"),
        "expected clap error: {stderr}"
    );
}

#[test]
fn every_stub_exits_nonzero_with_not_yet_implemented() {
    let cases: &[&[&str]] = &[
        &["init"],
        &["start", "POD-1"],
        &["set-ticket", "POD-1"],
        &["clear-ticket"],
        &["show"],
        &["protect", "main"],
        &["unprotect", "main"],
        &["config", "get", "branches.default_base"],
        &["doctor"],
        &["pr"],
        &["ticket"],
        &["hook", "prepare-commit-msg"],
    ];
    for args in cases {
        let assert = Command::cargo_bin("tix")
            .unwrap()
            .args(*args)
            .assert()
            .failure();
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        assert!(
            stderr.contains("not yet implemented"),
            "args {args:?}: expected stub message, got: {stderr}"
        );
    }
}

#[test]
fn start_accepts_ticket_description_and_base() {
    let assert = Command::cargo_bin("tix")
        .unwrap()
        .args(["start", "POD-1234", "fix-thing", "--base", "develop"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not yet implemented"), "stderr: {stderr}");
}

#[test]
fn start_accepts_flag_before_positional() {
    Command::cargo_bin("tix")
        .unwrap()
        .args(["start", "POD-1234", "--base", "develop", "fix-thing"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));
}

#[test]
fn config_set_accepts_scope_flag_and_value() {
    Command::cargo_bin("tix")
        .unwrap()
        .args([
            "config",
            "set",
            "branches.default_base",
            "develop",
            "--global",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));
}

#[test]
fn protect_accepts_global_or_repo_but_not_both() {
    Command::cargo_bin("tix")
        .unwrap()
        .args(["protect", "main", "--global"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));

    Command::cargo_bin("tix")
        .unwrap()
        .args(["protect", "main", "--global", "--repo"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("error:"));
}

#[test]
fn ticket_open_subcommand_parses() {
    Command::cargo_bin("tix")
        .unwrap()
        .args(["ticket", "open"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));
}

#[test]
fn hook_accepts_trailing_args_including_dashes() {
    Command::cargo_bin("tix")
        .unwrap()
        .args([
            "hook",
            "prepare-commit-msg",
            "/tmp/COMMIT_EDITMSG",
            "message",
            "--some-arg",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not yet implemented"));
}
