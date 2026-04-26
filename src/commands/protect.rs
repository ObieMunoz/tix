use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

use crate::cli::ScopeFlags;
use crate::config::Config;
use crate::git::Git;
use crate::util::paths::tix_config_dir;

pub fn protect(branch: &str, scope: ScopeFlags) -> Result<()> {
    let mut list = current_resolved_list()?;
    if !list.iter().any(|s| s == branch) {
        list.push(branch.to_string());
    }
    write_protected_list(scope, &list)?;
    print_resolved_list()
}

pub fn unprotect(branch: &str, scope: ScopeFlags) -> Result<()> {
    let mut list = current_resolved_list()?;
    let before = list.len();
    list.retain(|p| p != branch);
    if list.len() == before {
        eprintln!("warning: {branch:?} was not in the protected list");
    }
    write_protected_list(scope, &list)?;
    print_resolved_list()
}

fn current_resolved_list() -> Result<Vec<String>> {
    let git = Git::current();
    let cfg = Config::load(git.repo_root().ok().as_deref())?;
    Ok(cfg.branches.protected)
}

fn write_protected_list(scope: ScopeFlags, list: &[String]) -> Result<()> {
    let target = target_path(scope)?;

    let mut doc: toml::Value = if target.exists() {
        let content = std::fs::read_to_string(&target)
            .with_context(|| format!("reading {}", target.display()))?;
        content
            .parse()
            .with_context(|| format!("parsing {}", target.display()))?
    } else {
        toml::Value::Table(toml::Table::new())
    };

    let table = doc
        .as_table_mut()
        .ok_or_else(|| anyhow!("expected top-level table"))?;
    let branches = table
        .entry("branches".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let branches_table = branches
        .as_table_mut()
        .ok_or_else(|| anyhow!("[branches] is not a table"))?;
    let toml_list: Vec<toml::Value> = list
        .iter()
        .map(|s| toml::Value::String(s.clone()))
        .collect();
    branches_table.insert("protected".to_string(), toml::Value::Array(toml_list));

    let serialized = toml::to_string_pretty(&doc).context("serializing config")?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, &serialized)
        .with_context(|| format!("writing {}", target.display()))?;

    let g = if scope.repo {
        None
    } else {
        Some(target.as_path())
    };
    let r = if scope.repo {
        Some(target.as_path())
    } else {
        None
    };
    Config::load_from_paths(g, r).context("config write produced invalid TOML")?;
    Ok(())
}

fn target_path(scope: ScopeFlags) -> Result<PathBuf> {
    if scope.repo {
        let root = Git::current()
            .repo_root()
            .map_err(|_| anyhow!("--repo requires being inside a git repo"))?;
        Ok(root.join(".tix.toml"))
    } else {
        tix_config_dir()
            .map(|d| d.join("config.toml"))
            .ok_or_else(|| anyhow!("could not resolve config dir"))
    }
}

fn print_resolved_list() -> Result<()> {
    let git = Git::current();
    let cfg = Config::load(git.repo_root().ok().as_deref())?;
    println!("protected branches:");
    for p in &cfg.branches.protected {
        println!("  - {p}");
    }
    Ok(())
}
