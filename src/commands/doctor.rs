use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use regex::Regex;

use crate::commands::init::{HOOK_NAMES, shim_contents};
use crate::config::Config;
use crate::git::Git;
use crate::state::State;
use crate::util::paths::tix_config_dir;

const MIN_GIT_VERSION: (u32, u32) = (2, 30);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug)]
struct Check {
    name: &'static str,
    status: Status,
    summary: String,
    hint: Option<String>,
    detail: Option<String>,
}

impl Check {
    fn ok(name: &'static str, summary: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Ok,
            summary: summary.into(),
            hint: None,
            detail: None,
        }
    }
    fn warn(name: &'static str, summary: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Warn,
            summary: summary.into(),
            hint: Some(hint.into()),
            detail: None,
        }
    }
    fn fail(name: &'static str, summary: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Fail,
            summary: summary.into(),
            hint: Some(hint.into()),
            detail: None,
        }
    }
    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

pub fn run(verbose: bool) -> Result<ExitCode> {
    let git = Git::current();
    let mut checks = Vec::new();

    checks.push(check_git_version(&git));

    let hooks_dir = tix_config_dir().map(|d| d.join("hooks"));
    checks.push(check_hooks_path(&git, hooks_dir.as_deref())?);
    if let Some(hd) = hooks_dir.as_deref() {
        checks.push(check_shims(hd));
    }
    checks.push(check_global_config());

    let repo_root = git.repo_root().ok();
    if let Some(root) = repo_root.as_deref() {
        checks.push(check_local_hooks_path(&git, hooks_dir.as_deref())?);
        checks.push(check_repo_config(root));
        if let Ok(git_dir) = git.git_dir() {
            checks.push(check_state_file(&git_dir));
        }
        checks.push(check_default_base(&git, root));
    }

    checks.push(check_signing_key(&git));

    print_checks(&checks, verbose);

    let any_fail = checks.iter().any(|c| c.status == Status::Fail);
    Ok(if any_fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn print_checks(checks: &[Check], verbose: bool) {
    for c in checks {
        let tag = match c.status {
            Status::Ok => "OK",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        };
        println!("[{tag:>4}] {}: {}", c.name, c.summary);
        if c.status != Status::Ok
            && let Some(hint) = &c.hint
        {
            println!("       \u{2192} {hint}");
        }
        if verbose && let Some(detail) = &c.detail {
            println!("       {detail}");
        }
    }
}

fn check_git_version(git: &Git) -> Check {
    let raw = match git.version_string() {
        Ok(s) => s,
        Err(e) => {
            return Check::fail(
                "git available",
                "could not run `git --version`",
                "install git ≥ 2.30",
            )
            .with_detail(format!("{e:#}"));
        }
    };
    match parse_git_version(&raw) {
        Some((major, minor)) if (major, minor) >= MIN_GIT_VERSION => Check::ok(
            "git version",
            format!("{raw} (≥ {}.{})", MIN_GIT_VERSION.0, MIN_GIT_VERSION.1),
        ),
        Some((major, minor)) => Check::fail(
            "git version",
            format!("{major}.{minor} is older than required"),
            format!(
                "upgrade to git ≥ {}.{}",
                MIN_GIT_VERSION.0, MIN_GIT_VERSION.1
            ),
        ),
        None => Check::warn(
            "git version",
            format!("could not parse `{raw}`"),
            "verify `git --version` is sane",
        ),
    }
}

fn parse_git_version(s: &str) -> Option<(u32, u32)> {
    let re = Regex::new(r"git version (\d+)\.(\d+)").unwrap();
    let caps = re.captures(s)?;
    Some((caps[1].parse().ok()?, caps[2].parse().ok()?))
}

fn check_hooks_path(git: &Git, expected: Option<&Path>) -> Result<Check> {
    let value = git.get_global_config("core.hooksPath")?;
    let expected_str = expected.map(|p| p.to_string_lossy().to_string());
    Ok(match (value.as_deref(), expected_str.as_deref()) {
        (Some(actual), Some(want)) if actual == want => {
            Check::ok("core.hooksPath", actual.to_string())
        }
        (Some(actual), Some(want)) => Check::fail(
            "core.hooksPath",
            format!("set to {actual}, expected {want}"),
            "rerun `tix init --force` to point hooksPath at the managed dir",
        ),
        (Some(actual), None) => Check::warn(
            "core.hooksPath",
            format!("set to {actual} (could not resolve managed dir)"),
            "ensure $HOME or $XDG_CONFIG_HOME is set",
        ),
        (None, _) => Check::fail(
            "core.hooksPath",
            "not set",
            "run `tix init` to install hooks",
        ),
    })
}

fn check_local_hooks_path(git: &Git, expected: Option<&Path>) -> Result<Check> {
    let value = git.get_local_config("core.hooksPath")?;
    let expected_str = expected.map(|p| p.to_string_lossy().to_string());
    Ok(match (value.as_deref(), expected_str.as_deref()) {
        (None, _) => Check::ok("local hooksPath", "not set (global applies)"),
        (Some(actual), Some(want)) if actual == want => {
            Check::ok("local hooksPath", format!("{actual} (matches managed dir)"))
        }
        (Some(actual), _) => Check::fail(
            "local hooksPath",
            format!("local override hides our hooks: {actual}"),
            "run `git config --local --unset core.hooksPath`",
        ),
    })
}

fn check_shims(hooks_dir: &Path) -> Check {
    if !hooks_dir.exists() {
        return Check::fail(
            "hook shims",
            format!("hooks dir missing: {}", hooks_dir.display()),
            "run `tix init` to install shims",
        );
    }
    let mut missing = Vec::new();
    let mut wrong_content = Vec::new();
    let mut not_exec = Vec::new();

    for name in HOOK_NAMES {
        let path = hooks_dir.join(name);
        if !path.exists() {
            missing.push(*name);
            continue;
        }
        if !is_executable(&path) {
            not_exec.push(*name);
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) if contents == shim_contents(name) => {}
            Ok(_) => wrong_content.push(*name),
            Err(_) => missing.push(*name),
        }
    }

    if missing.is_empty() && wrong_content.is_empty() && not_exec.is_empty() {
        Check::ok(
            "hook shims",
            format!("all 3 present in {}", hooks_dir.display()),
        )
    } else {
        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!("missing: {}", missing.join(", ")));
        }
        if !not_exec.is_empty() {
            parts.push(format!("not executable: {}", not_exec.join(", ")));
        }
        if !wrong_content.is_empty() {
            parts.push(format!("modified: {}", wrong_content.join(", ")));
        }
        Check::fail(
            "hook shims",
            parts.join("; "),
            "run `tix init` to reinstall the managed shims",
        )
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn check_global_config() -> Check {
    let path = match tix_config_dir().map(|d| d.join("config.toml")) {
        Some(p) => p,
        None => {
            return Check::warn(
                "global config",
                "could not resolve config dir",
                "ensure $HOME or $XDG_CONFIG_HOME is set",
            );
        }
    };
    if !path.exists() {
        return Check::warn(
            "global config",
            format!("{} not present", path.display()),
            "run `tix init` to scaffold defaults",
        );
    }
    match Config::load_from_paths(Some(&path), None) {
        Ok(_) => Check::ok("global config", format!("{} parses", path.display())),
        Err(e) => Check::fail(
            "global config",
            format!("{} fails to parse", path.display()),
            "fix the TOML or rename the file and run `tix init`",
        )
        .with_detail(format!("{e:#}")),
    }
}

fn check_repo_config(repo_root: &Path) -> Check {
    let path = repo_root.join(".tix.toml");
    if !path.exists() {
        return Check::ok("repo config", "no .tix.toml (using global only)");
    }
    match Config::load_from_paths(None, Some(&path)) {
        Ok(_) => Check::ok("repo config", format!("{} parses", path.display())),
        Err(e) => Check::fail(
            "repo config",
            format!("{} fails to parse", path.display()),
            "fix the TOML or remove .tix.toml",
        )
        .with_detail(format!("{e:#}")),
    }
}

fn check_state_file(git_dir: &Path) -> Check {
    let path = git_dir.join("tix").join("state.json");
    if !path.exists() {
        return Check::ok("state file", "no state.json yet (no commits made)");
    }
    match State::load(git_dir) {
        Ok(_) => Check::ok("state file", format!("{} parses", path.display())),
        Err(e) => Check::fail(
            "state file",
            format!("{} fails to load", path.display()),
            "inspect or remove `<git-dir>/tix/state.json`",
        )
        .with_detail(format!("{e:#}")),
    }
}

fn check_default_base(git: &Git, repo_root: &Path) -> Check {
    let cfg = match Config::load(Some(repo_root)) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                "default_base",
                "config could not be loaded",
                "see `global config` and `repo config` checks above",
            )
            .with_detail(format!("{e:#}"));
        }
    };
    let base = &cfg.branches.default_base;
    let want = format!("refs/remotes/origin/{base}");
    match git.for_each_ref(&want) {
        Ok(refs) if !refs.is_empty() => {
            Check::ok("default_base", format!("origin/{base} resolves"))
        }
        Ok(_) => Check::warn(
            "default_base",
            format!("origin/{base} not found"),
            format!("`git fetch origin {base}` or set branches.default_base to a real branch"),
        ),
        Err(e) => Check::warn(
            "default_base",
            format!("could not query refs for origin/{base}"),
            "ensure the repo has a working `origin` remote",
        )
        .with_detail(format!("{e:#}")),
    }
}

fn check_signing_key(git: &Git) -> Check {
    let signing = git.get_global_config("commit.gpgsign").ok().flatten();
    if signing.as_deref() != Some("true") {
        return Check::ok("signing key", "commit.gpgsign not enabled (skipping)");
    }
    let key = git.get_global_config("user.signingKey").ok().flatten();
    match key {
        Some(k) if !k.is_empty() => Check::ok("signing key", format!("user.signingKey = {k}")),
        _ => Check::warn(
            "signing key",
            "commit.gpgsign=true but user.signingKey is unset",
            "set `git config --global user.signingKey <key-id>`",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_apple_git_version_string() {
        assert_eq!(
            parse_git_version("git version 2.42.0 (Apple Git-117)"),
            Some((2, 42))
        );
    }

    #[test]
    fn parses_plain_git_version() {
        assert_eq!(parse_git_version("git version 2.30.1"), Some((2, 30)));
        assert_eq!(parse_git_version("git version 3.0.0"), Some((3, 0)));
    }

    #[test]
    fn rejects_unrecognized_version_strings() {
        assert_eq!(parse_git_version("not git"), None);
        assert_eq!(parse_git_version(""), None);
    }
}
