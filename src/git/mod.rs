use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, anyhow};

#[derive(Debug, Default, Clone)]
pub struct Git {
    cwd: Option<PathBuf>,
    envs: Vec<(OsString, OsString)>,
}

impl Git {
    pub fn current() -> Self {
        Self::default()
    }

    pub fn at(path: impl AsRef<Path>) -> Self {
        Self {
            cwd: Some(path.as_ref().to_path_buf()),
            envs: Vec::new(),
        }
    }

    pub fn with_env(mut self, k: impl Into<OsString>, v: impl Into<OsString>) -> Self {
        self.envs.push((k.into(), v.into()));
        self
    }

    fn build_command(&self) -> Command {
        let mut cmd = Command::new("git");
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &self.envs {
            cmd.env(k, v);
        }
        cmd
    }

    fn try_run(&self, args: &[&str]) -> Result<Output> {
        let mut cmd = self.build_command();
        cmd.args(args);
        cmd.output()
            .with_context(|| format!("invoking `git {}`", args.join(" ")))
    }

    pub fn run(&self, args: &[&str]) -> Result<String> {
        let output = self.try_run(args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "`git {}` failed: {}",
                args.join(" "),
                stderr.trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn repo_root(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(self.run(&["rev-parse", "--show-toplevel"])?))
    }

    pub fn git_dir(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(
            self.run(&["rev-parse", "--absolute-git-dir"])?,
        ))
    }

    pub fn current_branch(&self) -> Result<String> {
        self.run(&["symbolic-ref", "--short", "HEAD"])
    }

    pub fn current_commit(&self) -> Result<String> {
        self.run(&["rev-parse", "HEAD"])
    }

    pub fn commit_subject(&self, sha: &str) -> Result<String> {
        self.run(&["log", "-1", "--format=%s", sha])
    }

    pub fn is_clean(&self) -> Result<bool> {
        Ok(self.run(&["status", "--porcelain"])?.is_empty())
    }

    pub fn for_each_ref(&self, pattern: &str) -> Result<Vec<String>> {
        let out = self.run(&["for-each-ref", "--format=%(refname)", pattern])?;
        if out.is_empty() {
            return Ok(Vec::new());
        }
        Ok(out.lines().map(str::to_string).collect())
    }

    pub fn rev_list_count(&self, range: &str) -> Result<u32> {
        let out = self.run(&["rev-list", "--count", range])?;
        out.parse::<u32>()
            .with_context(|| format!("parsing rev-list count: {out}"))
    }

    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>> {
        let output = self.try_run(&["merge-base", a, b])?;
        match output.status.code() {
            Some(0) => Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            )),
            Some(1) => Ok(None),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!(
                    "`git merge-base {a} {b}` failed: {}",
                    stderr.trim()
                ))
            }
        }
    }

    pub fn is_commit_on_remote(&self, sha: &str, remote_ref: &str) -> Result<bool> {
        let output = self.try_run(&["merge-base", "--is-ancestor", sha, remote_ref])?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!(
                    "`git merge-base --is-ancestor {sha} {remote_ref}` failed: {}",
                    stderr.trim()
                ))
            }
        }
    }

    pub fn fetch(&self, remote: &str, branch: &str) -> Result<()> {
        self.run(&["fetch", remote, branch])?;
        Ok(())
    }

    pub fn set_global_config(&self, key: &str, value: &str) -> Result<()> {
        self.run(&["config", "--global", key, value])?;
        Ok(())
    }

    pub fn get_global_config(&self, key: &str) -> Result<Option<String>> {
        let output = self.try_run(&["config", "--global", "--get", key])?;
        match output.status.code() {
            Some(0) => Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            )),
            Some(1) => Ok(None),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!(
                    "`git config --global --get {key}` failed: {}",
                    stderr.trim()
                ))
            }
        }
    }

    pub fn get_local_config(&self, key: &str) -> Result<Option<String>> {
        let output = self.try_run(&["config", "--local", "--get", key])?;
        match output.status.code() {
            Some(0) => Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            )),
            Some(1) => Ok(None),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!(
                    "`git config --local --get {key}` failed: {}",
                    stderr.trim()
                ))
            }
        }
    }

    pub fn version_string(&self) -> Result<String> {
        self.run(&["--version"])
    }

    pub fn unset_global_config(&self, key: &str) -> Result<()> {
        let output = self.try_run(&["config", "--global", "--unset", key])?;
        match output.status.code() {
            Some(0) | Some(5) => Ok(()),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!(
                    "`git config --global --unset {key}` failed: {}",
                    stderr.trim()
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_includes_command_and_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::at(dir.path());
        let err = git.current_branch().unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("symbolic-ref"),
            "expected command in error: {msg}"
        );
    }
}
