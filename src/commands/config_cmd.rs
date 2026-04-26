use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::ScopeFlags;
use crate::config::{Config, Source};
use crate::git::Git;
use crate::util::paths::tix_config_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyType {
    String,
    U32,
    StringList,
}

const SCHEMA: &[(&str, KeyType)] = &[
    ("ticket.pattern", KeyType::String),
    ("ticket.prefix_format", KeyType::String),
    ("branches.protected", KeyType::StringList),
    ("branches.default_base", KeyType::String),
    ("branches.start_prefix", KeyType::String),
    ("branches.naming_pattern", KeyType::String),
    ("branches.naming_enforcement", KeyType::String),
    ("push.stale_warn_threshold", KeyType::U32),
    ("integrations.ticket_url_template", KeyType::String),
    ("integrations.pr_provider", KeyType::String),
    ("integrations.pr_command", KeyType::String),
];

#[derive(Debug, Clone, Copy)]
pub enum ListScope {
    All,
    Global,
    Repo,
}

pub fn get(key: &str) -> Result<()> {
    lookup(key)?;
    let cfg = Config::load(repo_root().as_deref())?;
    let value = display_value(&cfg, key);
    let source = cfg.source(key).map(format_source).unwrap_or("?");
    println!("{key} = {value}  ({source})");
    Ok(())
}

pub fn list(scope: ListScope) -> Result<()> {
    match scope {
        ListScope::All => {
            let cfg = Config::load(repo_root().as_deref())?;
            for (key, _) in SCHEMA {
                let value = display_value(&cfg, key);
                let source = cfg.source(key).map(format_source).unwrap_or("?");
                println!("{key} = {value}  ({source})");
            }
        }
        ListScope::Global => list_file(&global_path()?, "global")?,
        ListScope::Repo => list_file(&repo_path()?, "repo")?,
    }
    Ok(())
}

pub fn set(
    key: &str,
    value: Option<String>,
    scope: ScopeFlags,
    append: Option<String>,
    remove: Option<String>,
) -> Result<()> {
    let kind = lookup(key)?;
    let target = if scope.repo {
        repo_path()?
    } else {
        global_path()?
    };

    let mut doc: toml::Value = if target.exists() {
        let content = std::fs::read_to_string(&target)
            .with_context(|| format!("reading {}", target.display()))?;
        content
            .parse()
            .with_context(|| format!("parsing {}", target.display()))?
    } else {
        toml::Value::Table(toml::Table::new())
    };

    match (value, append, remove) {
        (Some(v), None, None) => set_scalar(&mut doc, key, kind, &v)?,
        (None, Some(v), None) => list_append(&mut doc, key, kind, v)?,
        (None, None, Some(v)) => list_remove(&mut doc, key, kind, v)?,
        (None, None, None) => bail!("provide a VALUE, --append <V>, or --remove <V>"),
        _ => bail!("VALUE, --append, and --remove are mutually exclusive"),
    }

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

    println!("set {key} in {}", target.display());
    Ok(())
}

fn lookup(key: &str) -> Result<KeyType> {
    SCHEMA
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, t)| *t)
        .ok_or_else(|| anyhow!("unknown key `{key}`; known keys: {}", known_keys()))
}

fn known_keys() -> String {
    SCHEMA
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_source(s: Source) -> &'static str {
    match s {
        Source::Default => "default",
        Source::Global => "global",
        Source::Repo => "repo",
    }
}

fn set_scalar(doc: &mut toml::Value, key: &str, kind: KeyType, value: &str) -> Result<()> {
    let parsed = match kind {
        KeyType::String => toml::Value::String(value.to_string()),
        KeyType::U32 => {
            let n: u32 = value
                .parse()
                .map_err(|_| anyhow!("expected integer for `{key}`, got {value:?}"))?;
            toml::Value::Integer(n as i64)
        }
        KeyType::StringList => {
            bail!("`{key}` is a list — use --append <V> or --remove <V>, not a scalar value");
        }
    };
    set_dotted(doc, key, parsed)
}

fn list_append(doc: &mut toml::Value, key: &str, kind: KeyType, value: String) -> Result<()> {
    if kind != KeyType::StringList {
        bail!("`{key}` is not a list — use a scalar value");
    }
    if let Some(existing) = get_dotted_mut(doc, key) {
        if let Some(arr) = existing.as_array_mut() {
            if !arr.iter().any(|v| v.as_str() == Some(value.as_str())) {
                arr.push(toml::Value::String(value));
            }
            return Ok(());
        }
        bail!("`{key}` exists but is not a list");
    }
    set_dotted(
        doc,
        key,
        toml::Value::Array(vec![toml::Value::String(value)]),
    )
}

fn list_remove(doc: &mut toml::Value, key: &str, kind: KeyType, value: String) -> Result<()> {
    if kind != KeyType::StringList {
        bail!("`{key}` is not a list");
    }
    if let Some(existing) = get_dotted_mut(doc, key)
        && let Some(arr) = existing.as_array_mut()
    {
        let before = arr.len();
        arr.retain(|v| v.as_str() != Some(value.as_str()));
        if arr.len() == before {
            eprintln!("warning: {value:?} was not in `{key}`");
        }
    }
    Ok(())
}

fn set_dotted(doc: &mut toml::Value, key: &str, value: toml::Value) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();
    let mut cursor = doc;
    for (i, part) in parts.iter().enumerate() {
        let table = cursor
            .as_table_mut()
            .ok_or_else(|| anyhow!("expected table at `{}`", parts[..i].join(".")))?;
        if i == parts.len() - 1 {
            table.insert((*part).to_string(), value);
            return Ok(());
        }
        if !table.get(*part).is_some_and(toml::Value::is_table) {
            table.insert((*part).to_string(), toml::Value::Table(toml::Table::new()));
        }
        cursor = table.get_mut(*part).expect("just inserted");
    }
    Ok(())
}

fn get_dotted_mut<'a>(doc: &'a mut toml::Value, key: &str) -> Option<&'a mut toml::Value> {
    let parts: Vec<&str> = key.split('.').collect();
    let mut cursor = doc;
    for part in &parts {
        cursor = cursor.as_table_mut()?.get_mut(*part)?;
    }
    Some(cursor)
}

fn display_value(cfg: &Config, key: &str) -> String {
    match key {
        "ticket.pattern" => format!("{:?}", cfg.ticket.pattern),
        "ticket.prefix_format" => format!("{:?}", cfg.ticket.prefix_format),
        "branches.protected" => format_string_list(&cfg.branches.protected),
        "branches.default_base" => format!("{:?}", cfg.branches.default_base),
        "branches.start_prefix" => format!("{:?}", cfg.branches.start_prefix),
        "branches.naming_pattern" => format!("{:?}", cfg.branches.naming_pattern),
        "branches.naming_enforcement" => format!("{:?}", cfg.branches.naming_enforcement),
        "push.stale_warn_threshold" => cfg.push.stale_warn_threshold.to_string(),
        "integrations.ticket_url_template" => format!("{:?}", cfg.integrations.ticket_url_template),
        "integrations.pr_provider" => format!("{:?}", cfg.integrations.pr_provider),
        "integrations.pr_command" => format!("{:?}", cfg.integrations.pr_command),
        _ => "?".to_string(),
    }
}

fn format_string_list(list: &[String]) -> String {
    let inner: Vec<String> = list.iter().map(|s| format!("{s:?}")).collect();
    format!("[{}]", inner.join(", "))
}

fn list_file(path: &Path, label: &str) -> Result<()> {
    if !path.exists() {
        println!("(no {label} config: {})", path.display());
        return Ok(());
    }
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: toml::Value = content
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let mut entries = Vec::new();
    collect_entries(&parsed, "", &mut entries);
    if entries.is_empty() {
        println!("(empty {label} config: {})", path.display());
    } else {
        for (k, v) in entries {
            println!("{k} = {v}");
        }
    }
    Ok(())
}

fn collect_entries(v: &toml::Value, prefix: &str, out: &mut Vec<(String, String)>) {
    if let Some(table) = v.as_table() {
        for (k, val) in table {
            let key = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            if val.is_table() {
                collect_entries(val, &key, out);
            } else {
                out.push((key, format_toml_value(val)));
            }
        }
    }
}

fn format_toml_value(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => format!("{s:?}"),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Array(arr) => {
            let inner: Vec<String> = arr.iter().map(format_toml_value).collect();
            format!("[{}]", inner.join(", "))
        }
        other => other.to_string(),
    }
}

fn repo_root() -> Option<PathBuf> {
    Git::current().repo_root().ok()
}

fn repo_path() -> Result<PathBuf> {
    let root = Git::current()
        .repo_root()
        .map_err(|_| anyhow!("--repo requires being inside a git repo"))?;
    Ok(root.join(".tix.toml"))
}

fn global_path() -> Result<PathBuf> {
    tix_config_dir()
        .map(|d| d.join("config.toml"))
        .ok_or_else(|| anyhow!("could not resolve config dir; set $HOME or $XDG_CONFIG_HOME"))
}
