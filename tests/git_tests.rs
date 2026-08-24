use nanoom::git::{detect_git_root, resolve_base_commit, ComparisonMode, GitEvent, GitRepo};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn init_git_repo(path: &Path) {
    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .output()
        .unwrap();

    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.email")
        .arg("test@example.com")
        .output()
        .unwrap();

    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.name")
        .arg("Test User")
        .output()
        .unwrap();
}

fn commit_file(path: &Path, filename: &str, content: &str, message: &str) {
    fs::write(path.join(filename), content).unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("add")
        .arg(filename)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("commit")
        .arg("-m")
        .arg(message)
        .output()
        .unwrap();
}

#[test]
fn test_detect_git_root() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());

    let root = detect_git_root(dir.path()).unwrap();
    assert_eq!(root, dir.path());
}

#[test]
fn test_get_changed_files() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());

    commit_file(dir.path(), "file1.txt", "content1", "initial commit");
    commit_file(dir.path(), "file2.txt", "content2", "second commit");

    let repo = GitRepo::open(dir.path()).unwrap();
    let changed = repo.get_changed_files("HEAD~1", Some("HEAD")).unwrap();

    assert_eq!(changed.len(), 1);
    assert!(changed[0].ends_with("file2.txt"));
}

#[test]
fn test_get_all_files() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());

    commit_file(dir.path(), "file1.txt", "content1", "initial commit");
    commit_file(dir.path(), "file2.txt", "content2", "second commit");

    let repo = GitRepo::open(dir.path()).unwrap();
    let files = repo.get_all_files().unwrap();

    assert_eq!(files.len(), 2);
}

#[test]
fn test_get_merge_base() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());

    commit_file(dir.path(), "base.txt", "base", "base commit");
    let base_hash = std::process::Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .unwrap();
    let base_ref = String::from_utf8_lossy(&base_hash.stdout)
        .trim()
        .to_string();

    commit_file(dir.path(), "feature.txt", "feature", "feature commit");

    let repo = GitRepo::open(dir.path()).unwrap();
    let merge_base = repo.get_merge_base(&base_ref, "HEAD").unwrap();

    assert_eq!(merge_base, base_ref);
}

#[test]
fn test_comparison_mode_from_env() {
    std::env::set_var("COMPARISON", "merge-base");
    assert_eq!(ComparisonMode::from_env(), ComparisonMode::MergeBase);

    std::env::set_var("COMPARISON", "tip");
    assert_eq!(ComparisonMode::from_env(), ComparisonMode::Tip);

    std::env::remove_var("COMPARISON");
    assert_eq!(ComparisonMode::from_env(), ComparisonMode::MergeBase);
}

#[test]
fn test_resolve_base_commit_tip_mode() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    commit_file(dir.path(), "base.txt", "base", "base commit");
    commit_file(dir.path(), "feature.txt", "feature", "feature commit");

    std::process::Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .arg("branch")
        .arg("-M")
        .arg("main")
        .output()
        .unwrap();

    let repo = GitRepo::open(dir.path()).unwrap();
    let event = GitEvent::Push {
        ref_name: "main".to_string(),
    };

    let base = resolve_base_commit(&repo, &event, ComparisonMode::Tip).unwrap();
    assert!(!base.is_empty());
}
