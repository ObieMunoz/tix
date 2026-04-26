use anyhow::{Result, anyhow};

use crate::commands::init::HOOK_NAMES;
use crate::git::Git;
use crate::util::paths::tix_config_dir;

pub fn run(dry_run: bool, purge: bool) -> Result<()> {
    let config_dir = tix_config_dir().ok_or_else(|| {
        anyhow!("could not determine config directory; set $HOME or $XDG_CONFIG_HOME")
    })?;
    let hooks_dir = config_dir.join("hooks");
    let config_file = config_dir.join("config.toml");
    let hooks_path_str = hooks_dir.to_string_lossy().to_string();

    let git = Git::current();
    let existing = git.get_global_config("core.hooksPath")?;

    let owned_hooks: Vec<_> = HOOK_NAMES
        .iter()
        .map(|n| hooks_dir.join(n))
        .filter(|p| p.exists())
        .collect();
    let hooks_path_is_ours = matches!(existing.as_deref(), Some(p) if p == hooks_path_str);
    let hooks_path_is_other = existing.as_deref().is_some_and(|p| p != hooks_path_str);

    let mut plan = Vec::new();
    for p in &owned_hooks {
        plan.push(format!("remove {}", p.display()));
    }
    if hooks_path_is_ours {
        plan.push("unset git config --global core.hooksPath".to_string());
    } else if hooks_path_is_other {
        plan.push(format!(
            "leave core.hooksPath = `{}` (not managed by tix)",
            existing.as_deref().unwrap()
        ));
    }
    if purge {
        if config_file.exists() {
            plan.push(format!("remove {}", config_file.display()));
        }
        if config_dir.exists() {
            plan.push(format!("remove {} (if empty)", config_dir.display()));
        }
    } else if config_file.exists() {
        plan.push(format!(
            "leave {} (use --purge to remove)",
            config_file.display()
        ));
    }

    let actionable = !owned_hooks.is_empty()
        || hooks_path_is_ours
        || (purge && (config_file.exists() || config_dir.exists()));

    if !actionable {
        println!("tix uninstall: nothing to do");
        if hooks_path_is_other {
            println!(
                "  - core.hooksPath = `{}` (not managed by tix; left alone)",
                existing.as_deref().unwrap()
            );
        }
        return Ok(());
    }

    if dry_run {
        println!("tix uninstall (dry run) — would:");
        for line in &plan {
            println!("  - {line}");
        }
        return Ok(());
    }

    for p in &owned_hooks {
        std::fs::remove_file(p).map_err(|e| anyhow!("removing {}: {e}", p.display()))?;
    }
    if hooks_dir.exists() {
        let _ = std::fs::remove_dir(&hooks_dir);
    }

    if hooks_path_is_ours {
        git.unset_global_config("core.hooksPath")?;
    }

    if purge {
        if config_file.exists() {
            std::fs::remove_file(&config_file)
                .map_err(|e| anyhow!("removing {}: {e}", config_file.display()))?;
        }
        if config_dir.exists() {
            let _ = std::fs::remove_dir(&config_dir);
        }
    }

    println!("tix uninstall: complete");
    for line in &plan {
        println!("  - {line}");
    }
    println!();
    println!("Note: per-repo state in `<repo>/.git/tix/state.json` is left untouched.");
    Ok(())
}
