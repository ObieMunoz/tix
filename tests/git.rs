mod common;

use common::{canonical, commit, init_repo, init_with_origin, run_git};
use tix_git::git::Git;

#[test]
fn repo_root_returns_canonical_path() {
    let dir = init_repo();
    let path = canonical(dir.path());
    let git = Git::at(&path);
    assert_eq!(git.repo_root().unwrap(), path);
}

#[test]
fn git_dir_returns_dot_git() {
    let dir = init_repo();
    let path = canonical(dir.path());
    let git = Git::at(&path);
    assert_eq!(git.git_dir().unwrap(), path.join(".git"));
}

#[test]
fn current_branch_returns_initial_branch() {
    let dir = init_repo();
    let path = canonical(dir.path());
    commit(&path, "initial");
    assert_eq!(Git::at(&path).current_branch().unwrap(), "main");
}

#[test]
fn current_commit_returns_head_sha() {
    let dir = init_repo();
    let path = canonical(dir.path());
    let sha = commit(&path, "first");
    assert_eq!(Git::at(&path).current_commit().unwrap(), sha);
}

#[test]
fn commit_subject_returns_first_line() {
    let dir = init_repo();
    let path = canonical(dir.path());
    let sha = commit(&path, "POD-1 do the thing");
    assert_eq!(
        Git::at(&path).commit_subject(&sha).unwrap(),
        "POD-1 do the thing"
    );
}

#[test]
fn is_clean_true_for_fresh_repo() {
    let dir = init_repo();
    let path = canonical(dir.path());
    commit(&path, "first");
    assert!(Git::at(&path).is_clean().unwrap());
}

#[test]
fn is_clean_false_with_unstaged_change() {
    let dir = init_repo();
    let path = canonical(dir.path());
    commit(&path, "first");
    std::fs::write(path.join("dirty.txt"), "x").unwrap();
    assert!(!Git::at(&path).is_clean().unwrap());
}

#[test]
fn for_each_ref_lists_branches() {
    let dir = init_repo();
    let path = canonical(dir.path());
    commit(&path, "first");
    run_git(&path, &["branch", "feature/x"]);
    let refs = Git::at(&path).for_each_ref("refs/heads/").unwrap();
    assert!(refs.contains(&"refs/heads/main".to_string()));
    assert!(refs.contains(&"refs/heads/feature/x".to_string()));
}

#[test]
fn for_each_ref_empty_when_no_match() {
    let dir = init_repo();
    let path = canonical(dir.path());
    commit(&path, "first");
    let refs = Git::at(&path).for_each_ref("refs/tags/").unwrap();
    assert!(refs.is_empty());
}

#[test]
fn rev_list_count_returns_count() {
    let dir = init_repo();
    let path = canonical(dir.path());
    commit(&path, "first");
    commit(&path, "second");
    commit(&path, "third");
    assert_eq!(Git::at(&path).rev_list_count("HEAD").unwrap(), 3);
}

#[test]
fn merge_base_returns_common_ancestor() {
    let dir = init_repo();
    let path = canonical(dir.path());
    let base = commit(&path, "base");
    run_git(&path, &["checkout", "-b", "feature"]);
    commit(&path, "feature");
    run_git(&path, &["checkout", "main"]);
    commit(&path, "main2");
    let mb = Git::at(&path).merge_base("main", "feature").unwrap();
    assert_eq!(mb, Some(base));
}

#[test]
fn fetch_brings_remote_refs_into_local() {
    let (_bare, work) = init_with_origin();
    let work_path = canonical(work.path());
    commit(&work_path, "initial");
    run_git(&work_path, &["push", "origin", "main"]);

    let consumer = init_repo();
    let consumer_path = canonical(consumer.path());
    run_git(
        &consumer_path,
        &[
            "remote",
            "add",
            "origin",
            canonical(_bare.path()).to_str().unwrap(),
        ],
    );
    Git::at(&consumer_path).fetch("origin", "main").unwrap();
    let refs = Git::at(&consumer_path)
        .for_each_ref("refs/remotes/origin/")
        .unwrap();
    assert!(refs.contains(&"refs/remotes/origin/main".to_string()));
}

#[test]
fn is_commit_on_remote_true_after_push() {
    let (_bare, work) = init_with_origin();
    let path = canonical(work.path());
    let sha = commit(&path, "first");
    run_git(&path, &["push", "origin", "main"]);
    run_git(&path, &["fetch", "origin", "main"]);
    assert!(
        Git::at(&path)
            .is_commit_on_remote(&sha, "refs/remotes/origin/main")
            .unwrap()
    );
}

#[test]
fn is_commit_on_remote_false_for_local_only_commit() {
    let (_bare, work) = init_with_origin();
    let path = canonical(work.path());
    commit(&path, "pushed");
    run_git(&path, &["push", "origin", "main"]);
    run_git(&path, &["fetch", "origin", "main"]);
    let local_sha = commit(&path, "local-only");
    assert!(
        !Git::at(&path)
            .is_commit_on_remote(&local_sha, "refs/remotes/origin/main")
            .unwrap()
    );
}

#[test]
fn global_config_set_then_get_round_trips() {
    let dir = init_repo();
    let path = canonical(dir.path());
    let global_file = tempfile::NamedTempFile::new().unwrap();
    let git = Git::at(&path).with_env("GIT_CONFIG_GLOBAL", global_file.path());

    git.set_global_config("tix.testkey", "hello").unwrap();
    assert_eq!(
        git.get_global_config("tix.testkey").unwrap(),
        Some("hello".to_string())
    );
}

#[test]
fn global_config_get_returns_none_when_unset() {
    let dir = init_repo();
    let path = canonical(dir.path());
    let global_file = tempfile::NamedTempFile::new().unwrap();
    let git = Git::at(&path).with_env("GIT_CONFIG_GLOBAL", global_file.path());

    assert_eq!(git.get_global_config("tix.never_set").unwrap(), None);
}

#[test]
fn error_includes_failed_command_in_message() {
    let dir = tempfile::tempdir().unwrap();
    let git = Git::at(dir.path());
    let err = git.repo_root().unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("rev-parse"),
        "expected command in error: {msg}"
    );
}
