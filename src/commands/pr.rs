use std::process::Command as ProcCommand;

use anyhow::{Context, Result, anyhow, bail};

use crate::config::Config;
use crate::git::Git;
use crate::state::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    GitHub,
    GitLab,
    Bitbucket,
}

pub fn run() -> Result<()> {
    let git = Git::current();
    let repo_root = git.repo_root().map_err(|_| anyhow!("not in a git repo"))?;
    let branch = git
        .current_branch()
        .context("could not determine current branch")?;
    let cfg = Config::load(Some(&repo_root))?;

    let origin_url = git
        .run(&["remote", "get-url", "origin"])
        .context("could not get origin URL — is the `origin` remote configured?")?;

    let parsed = parse_origin(&origin_url)
        .ok_or_else(|| anyhow!("could not parse origin URL: {origin_url}"))?;
    let provider = detect_provider(&parsed.host, &cfg.integrations.pr_provider);

    if !has_upstream(&git, &branch) {
        bail!("branch '{branch}' has no upstream — run `git push -u origin {branch}` first");
    }

    let git_dir = git.git_dir()?;
    let state = State::load(&git_dir)?;
    let ticket = state.get_branch(&branch).and_then(|e| e.ticket.clone());

    if cfg.integrations.pr_command == "auto"
        && let Some(cli) = provider_cli(provider)
        && binary_on_path(cli)
    {
        return shell_out(cli, ticket.as_deref());
    }

    let url = build_pr_url(&parsed, provider, &branch);
    println!("{url}");
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginUrl {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

pub fn parse_origin(url: &str) -> Option<OriginUrl> {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches(".git");
    if let Some(rest) = trimmed.strip_prefix("git@")
        && let Some((host, path)) = rest.split_once(':')
        && let Some((owner, repo)) = path.split_once('/')
    {
        return Some(OriginUrl {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        });
    }
    if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("ssh://"))
    {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() == 3 {
            return Some(OriginUrl {
                host: parts[0].to_string(),
                owner: parts[1].to_string(),
                repo: parts[2].to_string(),
            });
        }
    }
    None
}

pub fn detect_provider(host: &str, fallback: &str) -> Provider {
    if host.contains("github.com") {
        Provider::GitHub
    } else if host.contains("gitlab.com") {
        Provider::GitLab
    } else if host.contains("bitbucket.org") {
        Provider::Bitbucket
    } else {
        match fallback {
            "gitlab" => Provider::GitLab,
            "bitbucket" => Provider::Bitbucket,
            _ => Provider::GitHub,
        }
    }
}

pub fn build_pr_url(origin: &OriginUrl, provider: Provider, branch: &str) -> String {
    let OriginUrl { host, owner, repo } = origin;
    match provider {
        Provider::GitHub => format!("https://{host}/{owner}/{repo}/compare/{branch}?expand=1"),
        Provider::GitLab => format!(
            "https://{host}/{owner}/{repo}/-/merge_requests/new?merge_request%5Bsource_branch%5D={branch}"
        ),
        Provider::Bitbucket => {
            format!("https://{host}/{owner}/{repo}/pull-requests/new?source={branch}")
        }
    }
}

fn provider_cli(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::GitHub => Some("gh"),
        Provider::GitLab => Some("glab"),
        Provider::Bitbucket => None,
    }
}

fn binary_on_path(name: &str) -> bool {
    ProcCommand::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn has_upstream(git: &Git, branch: &str) -> bool {
    git.run(&[
        "rev-parse",
        "--abbrev-ref",
        &format!("{branch}@{{upstream}}"),
    ])
    .is_ok()
}

fn shell_out(cli: &str, ticket_prefix: Option<&str>) -> Result<()> {
    let mut cmd = ProcCommand::new(cli);
    match cli {
        "gh" => {
            cmd.args(["pr", "create", "--web"]);
            if let Some(t) = ticket_prefix {
                cmd.args(["--title", &format!("{t} ")]);
            }
        }
        "glab" => {
            cmd.args(["mr", "create", "--web"]);
            if let Some(t) = ticket_prefix {
                cmd.args(["--title", &format!("{t} ")]);
            }
        }
        _ => bail!("unknown provider CLI: {cli}"),
    }
    let status = cmd.status().with_context(|| format!("invoking `{cli}`"))?;
    if !status.success() {
        bail!("`{cli}` exited non-zero");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_origin() {
        let url = "git@github.com:owner/repo.git";
        let o = parse_origin(url).unwrap();
        assert_eq!(o.host, "github.com");
        assert_eq!(o.owner, "owner");
        assert_eq!(o.repo, "repo");
    }

    #[test]
    fn parses_https_origin() {
        let url = "https://github.com/owner/repo.git";
        let o = parse_origin(url).unwrap();
        assert_eq!(o.host, "github.com");
        assert_eq!(o.owner, "owner");
        assert_eq!(o.repo, "repo");
    }

    #[test]
    fn parses_https_without_dot_git_suffix() {
        let url = "https://gitlab.com/group/project";
        let o = parse_origin(url).unwrap();
        assert_eq!(o.host, "gitlab.com");
        assert_eq!(o.owner, "group");
        assert_eq!(o.repo, "project");
    }

    #[test]
    fn detects_github() {
        assert_eq!(detect_provider("github.com", "gitlab"), Provider::GitHub);
    }

    #[test]
    fn detects_gitlab() {
        assert_eq!(detect_provider("gitlab.com", "github"), Provider::GitLab);
    }

    #[test]
    fn detects_bitbucket_subdomain() {
        assert_eq!(
            detect_provider("team.bitbucket.org", "github"),
            Provider::Bitbucket
        );
    }

    #[test]
    fn falls_back_to_config_pr_provider() {
        assert_eq!(
            detect_provider("git.internal.example.com", "gitlab"),
            Provider::GitLab
        );
    }

    #[test]
    fn builds_github_compare_url() {
        let o = OriginUrl {
            host: "github.com".into(),
            owner: "o".into(),
            repo: "r".into(),
        };
        assert_eq!(
            build_pr_url(&o, Provider::GitHub, "feat/POD-1"),
            "https://github.com/o/r/compare/feat/POD-1?expand=1"
        );
    }

    #[test]
    fn builds_gitlab_mr_url_with_url_encoded_brackets() {
        let o = OriginUrl {
            host: "gitlab.com".into(),
            owner: "o".into(),
            repo: "r".into(),
        };
        let url = build_pr_url(&o, Provider::GitLab, "feat/POD-1");
        assert!(url.contains("merge_request%5Bsource_branch%5D=feat/POD-1"));
    }
}
