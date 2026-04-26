use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command as ProcCommand;

use assert_cmd::Command;
use tempfile::{NamedTempFile, TempDir};

struct Env {
    home: TempDir,
    xdg: TempDir,
    git_global: NamedTempFile,
}

impl Env {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            xdg: tempfile::tempdir().unwrap(),
            git_global: NamedTempFile::new().unwrap(),
        }
    }

    fn hooks_dir(&self) -> std::path::PathBuf {
        self.xdg.path().join("tix").join("hooks")
    }

    fn config_file(&self) -> std::path::PathBuf {
        self.xdg.path().join("tix").join("config.toml")
    }

    fn run_init(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.run("init", args)
    }

    fn run_uninstall(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.run("uninstall", args)
    }

    fn run(&self, sub: &str, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("tix")
            .unwrap()
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.xdg.path())
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .arg(sub)
            .args(args)
            .assert()
    }

    fn preset_hooks_path(&self, value: &str) {
        ProcCommand::new("git")
            .env("GIT_CONFIG_GLOBAL", self.git_global.path())
            .args(["config", "--global", "core.hooksPath", value])
            .output()
            .unwrap();
    }

    fn read_global_config(&self) -> String {
        std::fs::read_to_string(self.git_global.path()).unwrap()
    }
}

fn assert_executable(path: &Path) {
    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o111,
        0o111,
        "expected executable bits on {}: mode={:o}",
        path.display(),
        mode
    );
}

#[test]
fn init_creates_hooks_config_and_sets_hookspath() {
    let env = Env::new();
    env.run_init(&[]).success();

    for hook in ["prepare-commit-msg", "pre-commit", "pre-push"] {
        let path = env.hooks_dir().join(hook);
        assert!(path.exists(), "hook missing: {}", path.display());
        assert_executable(&path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("#!/bin/sh\n"));
        assert!(
            content.contains(&format!("exec tix hook {hook} \"$@\"")),
            "hook contents wrong:\n{content}"
        );
        assert!(
            content.contains("command -v tix"),
            "missing presence-check:\n{content}"
        );
    }

    assert!(env.config_file().exists());
    let cfg = std::fs::read_to_string(env.config_file()).unwrap();
    assert!(cfg.contains("[ticket]"));
    assert!(cfg.contains("[branches]"));

    let global = env.read_global_config();
    assert!(global.contains("hooksPath"));
    assert!(global.contains(env.hooks_dir().to_str().unwrap()));
}

#[test]
fn init_is_idempotent_and_preserves_user_edited_config() {
    let env = Env::new();
    env.run_init(&[]).success();

    std::fs::write(env.config_file(), "# user customization\n").unwrap();
    env.run_init(&[]).success();

    assert_eq!(
        std::fs::read_to_string(env.config_file()).unwrap(),
        "# user customization\n",
        "init should not overwrite an existing config"
    );
}

#[test]
fn init_refuses_when_hookspath_set_to_other_dir() {
    let env = Env::new();
    env.preset_hooks_path("/tmp/elsewhere");

    env.run_init(&[])
        .failure()
        .stderr(predicates::str::contains("/tmp/elsewhere"))
        .stderr(predicates::str::contains("--force"));

    assert!(
        !env.hooks_dir().exists(),
        "hooks dir should not be created when refused"
    );
}

#[test]
fn init_force_overrides_existing_hookspath() {
    let env = Env::new();
    env.preset_hooks_path("/tmp/elsewhere");

    env.run_init(&["--force"]).success();
    let global = env.read_global_config();
    assert!(!global.contains("/tmp/elsewhere"));
    assert!(global.contains(env.hooks_dir().to_str().unwrap()));
}

#[test]
fn init_dry_run_writes_nothing() {
    let env = Env::new();
    let assert = env.run_init(&["--dry-run"]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("dry run"));
    assert!(!env.hooks_dir().exists());
    assert!(!env.config_file().exists());
    assert!(env.read_global_config().is_empty());
}

#[test]
fn uninstall_after_init_removes_hooks_and_unsets_hookspath() {
    let env = Env::new();
    env.run_init(&[]).success();
    assert!(env.hooks_dir().join("prepare-commit-msg").exists());
    assert!(env.read_global_config().contains("hooksPath"));

    env.run_uninstall(&[]).success();

    for hook in ["prepare-commit-msg", "pre-commit", "pre-push"] {
        assert!(
            !env.hooks_dir().join(hook).exists(),
            "hook should be removed: {hook}"
        );
    }
    assert!(
        !env.hooks_dir().exists(),
        "hooks dir should be empty + removed"
    );
    assert!(
        env.config_file().exists(),
        "config.toml should be preserved without --purge"
    );
    assert!(
        !env.read_global_config().contains("hooksPath"),
        "core.hooksPath should be unset"
    );
}

#[test]
fn uninstall_purge_also_removes_config_toml_and_dir() {
    let env = Env::new();
    env.run_init(&[]).success();
    env.run_uninstall(&["--purge"]).success();

    assert!(!env.config_file().exists(), "config.toml should be gone");
    assert!(
        !env.xdg.path().join("tix").exists(),
        "tix config dir should be gone"
    );
}

#[test]
fn uninstall_on_clean_machine_is_a_clean_no_op() {
    let env = Env::new();
    let assert = env.run_uninstall(&[]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("nothing to do"), "stdout: {stdout}");
}

#[test]
fn uninstall_leaves_other_users_hookspath_alone() {
    let env = Env::new();
    env.preset_hooks_path("/tmp/elsewhere");
    let assert = env.run_uninstall(&[]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("/tmp/elsewhere"), "stdout: {stdout}");
    assert!(
        env.read_global_config().contains("/tmp/elsewhere"),
        "must not unset another tool's hooksPath"
    );
}

#[test]
fn uninstall_dry_run_makes_no_changes() {
    let env = Env::new();
    env.run_init(&[]).success();

    let global_before = env.read_global_config();
    let assert = env.run_uninstall(&["--dry-run"]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("dry run"), "stdout: {stdout}");

    assert!(env.hooks_dir().join("prepare-commit-msg").exists());
    assert_eq!(env.read_global_config(), global_before);
}

#[test]
fn uninstall_preserves_unrelated_files_in_hooks_dir() {
    let env = Env::new();
    env.run_init(&[]).success();
    let custom = env.hooks_dir().join("custom-hook");
    std::fs::write(&custom, "#!/bin/sh\necho custom\n").unwrap();

    env.run_uninstall(&[]).success();
    assert!(custom.exists(), "user-added file must be preserved");
    assert!(env.hooks_dir().exists(), "hooks dir kept since not empty");
}

#[test]
fn shim_is_silent_no_op_when_tix_absent() {
    let env = Env::new();
    env.run_init(&[]).success();
    let shim = env.hooks_dir().join("prepare-commit-msg");

    let output = ProcCommand::new(&shim)
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "shim should exit 0 when tix absent (got {:?}, stderr: {})",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
