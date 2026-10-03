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

    let base = resolve_base_commit(&repo, &event, ComparisonMode::Tip, 2048).unwrap();
    assert!(!base.is_empty());
}

#[test]
fn shallow_linked_worktree_fetches_missing_base_history() {
    let origin = tempdir().unwrap();
    init_git_repo(origin.path());
    commit_file(origin.path(), "base.txt", "base", "base");
    let base_output = std::process::Command::new("git")
        .arg("-C")
        .arg(origin.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let base = String::from_utf8(base_output.stdout)
        .unwrap()
        .trim()
        .to_owned();
    commit_file(origin.path(), "next.txt", "next", "next");
    let clone = tempdir().unwrap();
    let checkout = clone.path().join("checkout");
    let output = std::process::Command::new("git")
        .args(["clone", "--no-local", "--depth=1"])
        .arg(origin.path())
        .arg(&checkout)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let linked = clone.path().join("linked");
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&checkout)
        .args(["worktree", "add", "--detach"])
        .arg(&linked)
        .arg("HEAD")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(linked.join(".git").is_file());
    let repo = GitRepo::open(&linked).unwrap();
    assert!(repo.is_shallow().unwrap());
    let event = GitEvent::PullRequest {
        base_ref: base.clone(),
        head_ref: "HEAD".into(),
    };
    assert_eq!(
        resolve_base_commit(&repo, &event, ComparisonMode::MergeBase, 32).unwrap(),
        base
    );
    assert!(!repo.is_shallow().unwrap());
    assert_eq!(
        repo.get_changed_files_from_tip(&base, Some("HEAD"))
            .unwrap(),
        vec![linked.canonicalize().unwrap().join("next.txt")]
    );
}

#[test]
fn git_file_lists_preserve_unicode_and_embedded_separators() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    commit_file(dir.path(), "base.txt", "base", "base");
    let names = vec!["한글 파일.txt"];
    #[cfg(not(windows))]
    let names = [names, vec!["line\nbreak.txt", "quote\"tab\t.txt"]].concat();
    for name in &names {
        fs::write(dir.path().join(name), "change").unwrap();
    }
    let git = |args: &[&str]| {
        let result = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    git(&["add", "."]);
    git(&["commit", "-m", "unusual names", "--no-gpg-sign"]);
    let repo = GitRepo::open(dir.path()).unwrap();
    let canonical_root = dir.path().canonicalize().unwrap();
    let mut expected: Vec<_> = names.iter().map(|name| canonical_root.join(name)).collect();
    expected.sort();
    for mut changed in [
        repo.get_changed_files("HEAD~1", Some("HEAD")).unwrap(),
        repo.get_changed_files_from_tip("HEAD~1", Some("HEAD"))
            .unwrap(),
    ] {
        changed.sort();
        assert_eq!(changed, expected);
    }
    let mut all = repo.get_all_files().unwrap();
    expected.push(canonical_root.join("base.txt"));
    all.sort();
    expected.sort();
    assert_eq!(all, expected);
}
