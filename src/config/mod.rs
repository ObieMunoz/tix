use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Default,
    Global,
    Repo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketConfig {
    pub pattern: String,
    pub prefix_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchesConfig {
    pub protected: Vec<String>,
    pub default_base: String,
    pub start_prefix: String,
    pub naming_pattern: String,
    pub naming_enforcement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushConfig {
    pub stale_warn_threshold: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationsConfig {
    pub ticket_url_template: String,
    pub pr_provider: String,
    pub pr_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub ticket: TicketConfig,
    pub branches: BranchesConfig,
    pub push: PushConfig,
    pub integrations: IntegrationsConfig,
    sources: HashMap<String, Source>,
}

impl Config {
    pub fn defaults() -> Self {
        let mut sources = HashMap::new();
        for f in ALL_FIELDS {
            sources.insert((*f).to_string(), Source::Default);
        }
        Self {
            ticket: TicketConfig {
                pattern: r"^[A-Z]+-\d+$".to_string(),
                prefix_format: "{ticket} {message}".to_string(),
            },
            branches: BranchesConfig {
                protected: vec![
                    "main".to_string(),
                    "master".to_string(),
                    "develop".to_string(),
                    "release/*".to_string(),
                ],
                default_base: "main".to_string(),
                start_prefix: "feature".to_string(),
                naming_pattern: r"^(feature|bugfix|hotfix|chore)/[A-Z]+-\d+(-.+)?$".to_string(),
                naming_enforcement: "warn".to_string(),
            },
            push: PushConfig {
                stale_warn_threshold: 50,
            },
            integrations: IntegrationsConfig {
                ticket_url_template: String::new(),
                pr_provider: "github".to_string(),
                pr_command: "auto".to_string(),
            },
            sources,
        }
    }

    pub fn source(&self, field: &str) -> Option<Source> {
        self.sources.get(field).copied()
    }

    pub fn load(repo_root: Option<&Path>) -> Result<Self> {
        let global = global_config_path();
        let repo = repo_root.map(|r| r.join(".tix.toml"));
        Self::load_from_paths(global.as_deref(), repo.as_deref())
    }

    pub fn load_from_paths(global: Option<&Path>, repo: Option<&Path>) -> Result<Self> {
        let mut c = Self::defaults();
        if let Some(p) = global
            && p.exists()
        {
            let raw = parse_file(p)?;
            c.overlay(raw, Source::Global);
        }
        if let Some(p) = repo
            && p.exists()
        {
            let raw = parse_file(p)?;
            c.overlay(raw, Source::Repo);
        }
        Ok(c)
    }

    fn overlay(&mut self, raw: RawConfig, src: Source) {
        if let Some(t) = raw.ticket {
            if let Some(v) = t.pattern {
                self.ticket.pattern = v;
                self.mark("ticket.pattern", src);
            }
            if let Some(v) = t.prefix_format {
                self.ticket.prefix_format = v;
                self.mark("ticket.prefix_format", src);
            }
        }
        if let Some(b) = raw.branches {
            if let Some(v) = b.protected {
                self.branches.protected = v;
                self.mark("branches.protected", src);
            }
            if let Some(v) = b.default_base {
                self.branches.default_base = v;
                self.mark("branches.default_base", src);
            }
            if let Some(v) = b.start_prefix {
                self.branches.start_prefix = v;
                self.mark("branches.start_prefix", src);
            }
            if let Some(v) = b.naming_pattern {
                self.branches.naming_pattern = v;
                self.mark("branches.naming_pattern", src);
            }
            if let Some(v) = b.naming_enforcement {
                self.branches.naming_enforcement = v;
                self.mark("branches.naming_enforcement", src);
            }
        }
        if let Some(p) = raw.push
            && let Some(v) = p.stale_warn_threshold
        {
            self.push.stale_warn_threshold = v;
            self.mark("push.stale_warn_threshold", src);
        }
        if let Some(i) = raw.integrations {
            if let Some(v) = i.ticket_url_template {
                self.integrations.ticket_url_template = v;
                self.mark("integrations.ticket_url_template", src);
            }
            if let Some(v) = i.pr_provider {
                self.integrations.pr_provider = v;
                self.mark("integrations.pr_provider", src);
            }
            if let Some(v) = i.pr_command {
                self.integrations.pr_command = v;
                self.mark("integrations.pr_command", src);
            }
        }
    }

    fn mark(&mut self, field: &str, src: Source) {
        self.sources.insert(field.to_string(), src);
    }
}

fn global_config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("tix").join("config.toml"))
}

fn parse_file(path: &Path) -> Result<RawConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file: {}", path.display()))?;
    toml::from_str::<RawConfig>(&content)
        .with_context(|| format!("parsing config file: {}", path.display()))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    ticket: Option<RawTicket>,
    branches: Option<RawBranches>,
    push: Option<RawPush>,
    integrations: Option<RawIntegrations>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTicket {
    pattern: Option<String>,
    prefix_format: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBranches {
    protected: Option<Vec<String>>,
    default_base: Option<String>,
    start_prefix: Option<String>,
    naming_pattern: Option<String>,
    naming_enforcement: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPush {
    stale_warn_threshold: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIntegrations {
    ticket_url_template: Option<String>,
    pr_provider: Option<String>,
    pr_command: Option<String>,
}

const ALL_FIELDS: &[&str] = &[
    "ticket.pattern",
    "ticket.prefix_format",
    "branches.protected",
    "branches.default_base",
    "branches.start_prefix",
    "branches.naming_pattern",
    "branches.naming_enforcement",
    "push.stale_warn_threshold",
    "integrations.ticket_url_template",
    "integrations.pr_provider",
    "integrations.pr_command",
];

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.path().join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn defaults_match_spec() {
        let c = Config::defaults();
        assert_eq!(c.ticket.pattern, r"^[A-Z]+-\d+$");
        assert_eq!(c.ticket.prefix_format, "{ticket} {message}");
        assert_eq!(
            c.branches.protected,
            vec!["main", "master", "develop", "release/*"]
        );
        assert_eq!(c.branches.default_base, "main");
        assert_eq!(c.branches.start_prefix, "feature");
        assert_eq!(c.branches.naming_enforcement, "warn");
        assert_eq!(c.push.stale_warn_threshold, 50);
        assert_eq!(c.integrations.ticket_url_template, "");
        assert_eq!(c.integrations.pr_provider, "github");
        assert_eq!(c.integrations.pr_command, "auto");
    }

    #[test]
    fn defaults_have_default_source_for_every_field() {
        let c = Config::defaults();
        for f in ALL_FIELDS {
            assert_eq!(c.source(f), Some(Source::Default), "field {f}");
        }
    }

    #[test]
    fn unknown_field_returns_none() {
        let c = Config::defaults();
        assert_eq!(c.source("nope.nope"), None);
    }

    #[test]
    fn missing_files_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let g = dir.path().join("global.toml");
        let r = dir.path().join("repo.toml");
        let c = Config::load_from_paths(Some(&g), Some(&r)).unwrap();
        assert_eq!(c, Config::defaults());
    }

    #[test]
    fn global_overrides_default() {
        let dir = tempfile::tempdir().unwrap();
        let g = write(
            &dir,
            "global.toml",
            "[branches]\ndefault_base = \"develop\"\n",
        );
        let c = Config::load_from_paths(Some(&g), None).unwrap();
        assert_eq!(c.branches.default_base, "develop");
        assert_eq!(c.branches.start_prefix, "feature");
        assert_eq!(c.source("branches.default_base"), Some(Source::Global));
        assert_eq!(c.source("branches.start_prefix"), Some(Source::Default));
    }

    #[test]
    fn repo_overrides_global() {
        let dir = tempfile::tempdir().unwrap();
        let g = write(
            &dir,
            "global.toml",
            "[branches]\ndefault_base = \"develop\"\nstart_prefix = \"feat\"\n",
        );
        let r = write(&dir, "repo.toml", "[branches]\ndefault_base = \"trunk\"\n");
        let c = Config::load_from_paths(Some(&g), Some(&r)).unwrap();
        assert_eq!(c.branches.default_base, "trunk");
        assert_eq!(c.branches.start_prefix, "feat");
        assert_eq!(c.source("branches.default_base"), Some(Source::Repo));
        assert_eq!(c.source("branches.start_prefix"), Some(Source::Global));
    }

    #[test]
    fn list_value_replaced_not_merged() {
        let dir = tempfile::tempdir().unwrap();
        let r = write(&dir, "repo.toml", "[branches]\nprotected = [\"trunk\"]\n");
        let c = Config::load_from_paths(None, Some(&r)).unwrap();
        assert_eq!(c.branches.protected, vec!["trunk"]);
    }

    #[test]
    fn malformed_toml_error_includes_path() {
        let dir = tempfile::tempdir().unwrap();
        let g = write(&dir, "global.toml", "this is = not = valid\n[[[ broken");
        let err = Config::load_from_paths(Some(&g), None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("global.toml"), "expected path in error: {msg}");
    }

    #[test]
    fn unknown_field_in_toml_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let g = write(&dir, "global.toml", "[branches]\nnonsense_field = \"x\"\n");
        let err = Config::load_from_paths(Some(&g), None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("global.toml"), "expected path: {msg}");
    }
}
