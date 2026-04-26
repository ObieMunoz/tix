use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchEntry {
    pub ticket: Option<String>,
    pub set_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub amended_through: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    #[serde(default)]
    pub branches: BTreeMap<String, BranchEntry>,
}

impl State {
    pub fn empty() -> Self {
        Self {
            version: CURRENT_VERSION,
            branches: BTreeMap::new(),
        }
    }

    pub fn load(git_dir: &Path) -> Result<Self> {
        let path = state_path(git_dir);
        if !path.exists() {
            return Ok(Self::empty());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading state file: {}", path.display()))?;
        let state: Self = serde_json::from_str(&content).with_context(|| {
            format!(
                "parsing state file: {} (run `tix doctor` for diagnostics)",
                path.display()
            )
        })?;
        if state.version != CURRENT_VERSION {
            return Err(anyhow!(
                "state file {} has unsupported version {} (expected {})",
                path.display(),
                state.version,
                CURRENT_VERSION
            ));
        }
        Ok(state)
    }

    pub fn save(&self, git_dir: &Path) -> Result<()> {
        let dir = git_dir.join("tix");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating state dir: {}", dir.display()))?;
        let final_path = dir.join("state.json");
        let tmp = dir.join("state.json.tmp");
        let json = serde_json::to_vec_pretty(self).context("serializing state")?;
        std::fs::write(&tmp, &json)
            .with_context(|| format!("writing state temp file: {}", tmp.display()))?;
        std::fs::rename(&tmp, &final_path)
            .with_context(|| format!("renaming {} → {}", tmp.display(), final_path.display()))?;
        Ok(())
    }

    pub fn get_branch(&self, name: &str) -> Option<&BranchEntry> {
        self.branches.get(name)
    }

    pub fn set_branch(&mut self, name: impl Into<String>, entry: BranchEntry) {
        self.branches.insert(name.into(), entry);
    }

    pub fn clear_branch(&mut self, name: &str) {
        self.branches.remove(name);
    }
}

fn state_path(git_dir: &Path) -> PathBuf {
    git_dir.join("tix").join("state.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn empty_returns_current_version_and_no_branches() {
        let s = State::empty();
        assert_eq!(s.version, CURRENT_VERSION);
        assert!(s.branches.is_empty());
    }

    #[test]
    fn load_returns_empty_when_file_absent() {
        let dir = tempfile::tempdir().unwrap();
        let s = State::load(dir.path()).unwrap();
        assert_eq!(s, State::empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = State::empty();
        s.set_branch(
            "feature/POD-1234-fix",
            BranchEntry {
                ticket: Some("POD-1234".to_string()),
                set_at: ts("2026-04-26T12:34:56Z"),
                amended_through: Some("abc123def456".to_string()),
            },
        );
        s.set_branch(
            "hotfix/scratch",
            BranchEntry {
                ticket: None,
                set_at: ts("2026-04-26T13:00:00Z"),
                amended_through: None,
            },
        );
        s.save(dir.path()).unwrap();
        let loaded = State::load(dir.path()).unwrap();
        assert_eq!(loaded, s);
    }

    #[test]
    fn save_creates_tix_directory_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join("tix").exists());
        State::empty().save(dir.path()).unwrap();
        assert!(dir.path().join("tix").join("state.json").exists());
    }

    #[test]
    fn save_leaves_no_tmp_file() {
        let dir = tempfile::tempdir().unwrap();
        State::empty().save(dir.path()).unwrap();
        assert!(!dir.path().join("tix").join("state.json.tmp").exists());
    }

    #[test]
    fn save_overwrites_previous_content() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = State::empty();
        s.set_branch(
            "feature/x",
            BranchEntry {
                ticket: Some("X-1".into()),
                set_at: Utc.timestamp_opt(0, 0).unwrap(),
                amended_through: None,
            },
        );
        s.save(dir.path()).unwrap();

        let mut s2 = State::empty();
        s2.set_branch(
            "feature/y",
            BranchEntry {
                ticket: Some("Y-2".into()),
                set_at: Utc.timestamp_opt(0, 0).unwrap(),
                amended_through: None,
            },
        );
        s2.save(dir.path()).unwrap();

        let loaded = State::load(dir.path()).unwrap();
        assert!(loaded.get_branch("feature/x").is_none());
        assert!(loaded.get_branch("feature/y").is_some());
    }

    #[test]
    fn set_get_clear_branch() {
        let mut s = State::empty();
        assert!(s.get_branch("feature/x").is_none());
        s.set_branch(
            "feature/x",
            BranchEntry {
                ticket: Some("X-1".into()),
                set_at: Utc.timestamp_opt(0, 0).unwrap(),
                amended_through: None,
            },
        );
        assert_eq!(
            s.get_branch("feature/x").unwrap().ticket.as_deref(),
            Some("X-1")
        );
        s.clear_branch("feature/x");
        assert!(s.get_branch("feature/x").is_none());
    }

    #[test]
    fn unknown_version_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tix");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("state.json"),
            r#"{"version": 999, "branches": {}}"#,
        )
        .unwrap();
        let err = State::load(dir.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("999") && msg.contains("state.json"),
            "expected version + path in error: {msg}"
        );
    }

    #[test]
    fn malformed_json_error_includes_path_and_doctor_hint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tix");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("state.json"), "not json {{{").unwrap();
        let err = State::load(dir.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("state.json"), "expected path: {msg}");
        assert!(msg.contains("tix doctor"), "expected doctor hint: {msg}");
    }

    #[test]
    fn amended_through_omitted_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = State::empty();
        s.set_branch(
            "feature/x",
            BranchEntry {
                ticket: None,
                set_at: ts("2026-04-26T12:34:56Z"),
                amended_through: None,
            },
        );
        s.save(dir.path()).unwrap();
        let raw = std::fs::read_to_string(dir.path().join("tix").join("state.json")).unwrap();
        assert!(
            !raw.contains("amended_through"),
            "expected amended_through absent in JSON: {raw}"
        );
    }
}
