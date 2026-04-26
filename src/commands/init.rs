use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::git::Git;
use crate::util::paths::tix_config_dir;

pub const HOOK_NAMES: &[&str] = &["prepare-commit-msg", "pre-commit", "pre-push"];

pub const DEFAULT_CONFIG_TOML: &str = r#"# tix global config — see SPEC.md for full reference

[ticket]
pattern = '^[A-Z]+-\d+$'
prefix_format = "{ticket} {message}"

[branches]
protected = ["main", "master", "develop", "release/*"]
default_base = "main"
start_prefix = "feature"
naming_pattern = '^(feature|bugfix|hotfix|chore)/[A-Z]+-\d+(-.+)?$'
naming_enforcement = "warn"

[push]
stale_warn_threshold = 50

[integrations]
ticket_url_template = ""
pr_provider = "github"
pr_command = "auto"
"#;

pub fn run(dry_run: bool, force: bool) -> Result<()> {
    let config_dir = tix_config_dir().ok_or_else(|| {
        anyhow!("could not determine config directory; set $HOME or $XDG_CONFIG_HOME")
    })?;
    let hooks_dir = config_dir.join("hooks");
    let config_file = config_dir.join("config.toml");

    let git = Git::current();
    let existing = git.get_global_config("core.hooksPath")?;
    let hooks_path_str = hooks_dir.to_string_lossy().to_string();

    let needs_set = match existing.as_deref() {
        None => true,
        Some(p) if p == hooks_path_str => false,
        Some(other) => {
            if !force {
                return Err(anyhow!(
                    "core.hooksPath is already set to `{other}`; rerun with --force to override"
                ));
            }
            true
        }
    };
    let needs_config = !config_file.exists();

    let plan = build_plan(
        &hooks_dir,
        &config_file,
        needs_set,
        needs_config,
        &hooks_path_str,
    );

    if dry_run {
        println!("tix init (dry run) — would:");
        for line in &plan {
            println!("  - {line}");
        }
        return Ok(());
    }

    std::fs::create_dir_all(&hooks_dir)
        .with_context(|| format!("creating hooks dir: {}", hooks_dir.display()))?;

    for name in HOOK_NAMES {
        write_shim(&hooks_dir.join(name), name)?;
    }

    if needs_set {
        git.set_global_config("core.hooksPath", &hooks_path_str)?;
    }

    if needs_config {
        std::fs::write(&config_file, DEFAULT_CONFIG_TOML)
            .with_context(|| format!("writing config: {}", config_file.display()))?;
    }

    println!("tix init: ready");
    for line in &plan {
        println!("  - {line}");
    }
    println!();
    println!("Next: cd into any git repo, make a commit, and you'll be prompted for a ticket.");
    Ok(())
}

fn build_plan(
    hooks_dir: &Path,
    config_file: &Path,
    needs_set: bool,
    needs_config: bool,
    hooks_path_str: &str,
) -> Vec<String> {
    let mut plan = Vec::new();
    for name in HOOK_NAMES {
        plan.push(format!("install hook {}", hooks_dir.join(name).display()));
    }
    if needs_set {
        plan.push(format!(
            "set git config --global core.hooksPath = {hooks_path_str}"
        ));
    } else {
        plan.push("core.hooksPath already pointing here".to_string());
    }
    if needs_config {
        plan.push(format!("scaffold {}", config_file.display()));
    } else {
        plan.push(format!(
            "{} already exists — leaving untouched",
            config_file.display()
        ));
    }
    plan
}

fn write_shim(path: &PathBuf, hook_name: &str) -> Result<()> {
    let content = shim_contents(hook_name);
    std::fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("chmod +x {}", path.display()))?;
    Ok(())
}

pub fn shim_contents(hook_name: &str) -> String {
    format!(
        "#!/bin/sh\ncommand -v tix > /dev/null 2>&1 || exit 0\nexec tix hook {hook_name} \"$@\"\n"
    )
}
