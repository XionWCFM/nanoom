use crate::error::{Error, Result};
use gix::{open, Repository};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct GitRepo {
    repo: Repository,
    workdir: PathBuf,
}

impl GitRepo {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = open(path).map_err(|e| Error::GitError(e.to_string()))?;
        let workdir = repo.workdir().unwrap_or(path).to_path_buf();
        Ok(Self { repo, workdir })
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    pub fn is_shallow(&self) -> Result<bool> {
        let shallow_file = self.workdir.join(".git/shallow");
        Ok(shallow_file.exists())
    }

    pub fn get_merge_base(&self, base: &str, head: &str) -> Result<String> {
        let base_id = self
            .repo
            .rev_parse_single(base)
            .map_err(|e| Error::GitError(e.to_string()))?;
        let head_id = self
            .repo
            .rev_parse_single(head)
            .map_err(|e| Error::GitError(e.to_string()))?;

        match self.repo.merge_base(base_id, head_id) {
            Ok(merge_base) => Ok(merge_base.to_string()),
            Err(_) => Err(Error::NoCommonAncestor {
                base: base.to_string(),
                head: head.to_string(),
            }),
        }
    }

    pub fn get_changed_files(&self, base: &str, head: Option<&str>) -> Result<Vec<PathBuf>> {
        let head_rev = head.unwrap_or("HEAD");
        self.get_changed_files_for_range(&format!("{}...{}", base, head_rev))
    }

    pub fn get_changed_files_from_tip(
        &self,
        base: &str,
        head: Option<&str>,
    ) -> Result<Vec<PathBuf>> {
        let head_rev = head.unwrap_or("HEAD");
        self.get_changed_files_for_range(&format!("{}..{}", base, head_rev))
    }

    fn get_changed_files_for_range(&self, range: &str) -> Result<Vec<PathBuf>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.workdir)
            .arg("diff")
            .arg("--name-only")
            .arg(range)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::GitError(format!("git diff failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<PathBuf> = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| self.workdir.join(l))
            .collect();

        Ok(files)
    }

    pub fn get_all_files(&self) -> Result<Vec<PathBuf>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.workdir)
            .arg("ls-files")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::GitError(format!("git ls-files failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<PathBuf> = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| self.workdir.join(l))
            .collect();

        Ok(files)
    }
}

pub fn detect_git_root(path: &Path) -> Result<PathBuf> {
    let repo = open(path).map_err(|e| Error::GitError(e.to_string()))?;
    Ok(repo.workdir().unwrap_or(path).to_path_buf())
}

#[derive(Debug, Clone)]
pub enum GitEvent {
    Push { ref_name: String },
    PullRequest { base_ref: String, head_ref: String },
    MergeGroup { base_ref: String, head_ref: String },
}

impl GitEvent {
    pub fn base_ref(&self) -> &str {
        match self {
            GitEvent::Push { ref_name } => ref_name,
            GitEvent::PullRequest { base_ref, .. } => base_ref,
            GitEvent::MergeGroup { base_ref, .. } => base_ref,
        }
    }

    pub fn head_ref(&self) -> &str {
        match self {
            GitEvent::Push { .. } => "HEAD",
            GitEvent::PullRequest { head_ref, .. } => head_ref,
            GitEvent::MergeGroup { head_ref, .. } => head_ref,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonMode {
    MergeBase,
    Tip,
}

impl ComparisonMode {
    pub fn from_env() -> Self {
        match std::env::var("COMPARISON").as_deref() {
            Ok("tip") => ComparisonMode::Tip,
            _ => ComparisonMode::MergeBase,
        }
    }
}

pub fn resolve_base_commit(
    repo: &GitRepo,
    event: &GitEvent,
    mode: ComparisonMode,
) -> Result<String> {
    let base_ref = event.base_ref();
    let head_ref = event.head_ref();

    match mode {
        ComparisonMode::Tip => {
            let base_id = repo
                .repo
                .rev_parse_single(base_ref)
                .map_err(|e| Error::GitError(e.to_string()))?;
            Ok(base_id.to_string())
        }
        ComparisonMode::MergeBase => {
            let result = try_merge_base_with_deepen(repo, base_ref, head_ref);
            match result {
                Ok(base) => Ok(base),
                Err(e) => {
                    if repo.is_shallow()? {
                        Err(Error::ShallowRepository)
                    } else {
                        Err(e)
                    }
                }
            }
        }
    }
}

fn try_merge_base_with_deepen(repo: &GitRepo, base_ref: &str, head_ref: &str) -> Result<String> {
    let max_attempts = 3;
    let mut fetch_depth = 128;

    for attempt in 0..max_attempts {
        match repo.get_merge_base(base_ref, head_ref) {
            Ok(base) => return Ok(base),
            Err(Error::NoCommonAncestor { .. }) => {
                if attempt == max_attempts - 1 {
                    return Err(Error::NoCommonAncestor {
                        base: base_ref.to_string(),
                        head: head_ref.to_string(),
                    });
                }
                deepen_fetch(repo, fetch_depth)?;
                fetch_depth *= 2;
            }
            Err(e) => return Err(e),
        }
    }

    Err(Error::NoCommonAncestor {
        base: base_ref.to_string(),
        head: head_ref.to_string(),
    })
}

fn deepen_fetch(repo: &GitRepo, depth: usize) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo.workdir)
        .arg("fetch")
        // Preserve the bounded checkout history and add only the next chunk.
        // `--depth` replaces the shallow boundary; `--deepen` is cumulative.
        .arg("--deepen")
        .arg(depth.to_string())
        .arg("origin")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::GitError(format!(
            "Failed to deepen fetch: {}",
            stderr
        )));
    }

    Ok(())
}

pub fn detect_default_branch(repo: &GitRepo) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo.workdir)
        .arg("ls-remote")
        .arg("--symref")
        .arg("origin")
        .arg("HEAD")
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(stripped) = line.strip_prefix("ref: refs/heads/") {
                if let Some(branch) = stripped.split_whitespace().next() {
                    return Ok(branch.to_string());
                }
            }
        }
    }

    Ok("main".to_string())
}

pub fn is_fork_pr() -> bool {
    std::env::var("GITHUB_REPOSITORY")
        .map(|repo| {
            std::env::var("GITHUB_HEAD_REPOSITORY")
                .map(|head_repo| repo != head_repo)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub fn get_origin_url(repo: &GitRepo) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo.workdir)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(Error::GitError("Failed to get origin URL".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn rev_parse(dir: &Path, rev: &str) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .arg("rev-parse")
            .arg(rev)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "init"]);
        dir
    }

    fn make_orphan_branch(dir: &Path) {
        git(dir, &["checkout", "--orphan", "isolated"]);
        std::fs::write(dir.join("b.txt"), "other root").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-m", "orphan"]);
    }

    #[test]
    fn test_git_repo_open_and_workdir() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        assert_eq!(repo.workdir(), dir.path());
    }

    #[test]
    fn test_git_repo_open_non_repo_errors() {
        let dir = tempfile::tempdir().unwrap();
        assert!(GitRepo::open(dir.path()).is_err());
    }

    #[test]
    fn test_detect_git_root_valid_and_invalid() {
        let dir = init_repo();
        assert_eq!(detect_git_root(dir.path()).unwrap(), dir.path());

        let empty = tempfile::tempdir().unwrap();
        assert!(detect_git_root(empty.path()).is_err());
    }

    #[test]
    fn test_is_shallow_false_then_true() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        assert!(!repo.is_shallow().unwrap());
        std::fs::write(dir.path().join(".git/shallow"), "").unwrap();
        assert!(repo.is_shallow().unwrap());
    }

    #[test]
    fn test_get_merge_base_same_history() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        let head = rev_parse(dir.path(), "HEAD");
        assert_eq!(repo.get_merge_base("main", "HEAD").unwrap(), head);
    }

    #[test]
    fn test_get_merge_base_invalid_ref_errors() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        assert!(matches!(
            repo.get_merge_base("does-not-exist", "HEAD"),
            Err(Error::GitError(_))
        ));
    }

    #[test]
    fn test_get_merge_base_orphan_histories_no_common_ancestor() {
        let dir = init_repo();
        make_orphan_branch(dir.path());
        let repo = GitRepo::open(dir.path()).unwrap();
        assert!(matches!(
            repo.get_merge_base("main", "isolated"),
            Err(Error::NoCommonAncestor { .. })
        ));
    }

    #[test]
    fn test_get_changed_files_between_commits() {
        let dir = init_repo();
        let base = rev_parse(dir.path(), "HEAD");
        std::fs::write(dir.path().join("a.txt"), "changed").unwrap();
        std::fs::write(dir.path().join("new.txt"), "added").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "second"]);

        let repo = GitRepo::open(dir.path()).unwrap();
        let mut files = repo.get_changed_files(&base, None).unwrap();
        files.sort();
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.txt".to_string(), "new.txt".to_string()]);
    }

    #[test]
    fn tip_comparison_includes_base_branch_divergence() {
        let dir = init_repo();
        git(dir.path(), &["checkout", "-b", "feature"]);
        git(dir.path(), &["checkout", "main"]);
        std::fs::write(dir.path().join("main.txt"), "main").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "main change"]);
        git(dir.path(), &["checkout", "feature"]);
        std::fs::write(dir.path().join("feature.txt"), "feature").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "feature change"]);

        let repo = GitRepo::open(dir.path()).unwrap();
        let merge_base_files = repo.get_changed_files("main", Some("feature")).unwrap();
        let tip_files = repo
            .get_changed_files_from_tip("main", Some("feature"))
            .unwrap();
        assert_eq!(merge_base_files.len(), 1);
        assert_eq!(tip_files.len(), 2);
    }

    #[test]
    fn test_get_changed_files_invalid_base_errors() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        match repo.get_changed_files("no-such-ref", None) {
            Err(Error::GitError(msg)) => assert!(msg.contains("git diff failed")),
            other => panic!("expected GitError, got {:?}", other),
        }
    }

    #[test]
    fn test_get_all_files_lists_tracked_files() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        let files = repo.get_all_files().unwrap();
        assert!(files.iter().any(|f| f.file_name().unwrap() == "a.txt"));
    }

    #[test]
    #[serial]
    fn test_resolve_base_commit_tip_mode() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        let event = GitEvent::Push {
            ref_name: "main".to_string(),
        };
        let base = resolve_base_commit(&repo, &event, ComparisonMode::Tip).unwrap();
        assert_eq!(base, rev_parse(dir.path(), "main"));
    }

    #[test]
    #[serial]
    fn test_resolve_base_commit_tip_mode_invalid_ref() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        let event = GitEvent::Push {
            ref_name: "nope".to_string(),
        };
        assert!(matches!(
            resolve_base_commit(&repo, &event, ComparisonMode::Tip),
            Err(Error::GitError(_))
        ));
    }

    #[test]
    #[serial]
    fn test_resolve_base_commit_merge_base_mode() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        let event = GitEvent::Push {
            ref_name: "main".to_string(),
        };
        let base = resolve_base_commit(&repo, &event, ComparisonMode::MergeBase).unwrap();
        assert_eq!(base, rev_parse(dir.path(), "HEAD"));
    }

    #[test]
    #[serial]
    fn test_resolve_base_commit_shallow_reports_shallow_error() {
        let dir = init_repo();
        make_orphan_branch(dir.path());
        std::fs::write(dir.path().join(".git/shallow"), "").unwrap();
        let repo = GitRepo::open(dir.path()).unwrap();
        let event = GitEvent::PullRequest {
            base_ref: "main".to_string(),
            head_ref: "isolated".to_string(),
        };
        assert!(matches!(
            resolve_base_commit(&repo, &event, ComparisonMode::MergeBase),
            Err(Error::ShallowRepository)
        ));
    }

    #[test]
    #[serial]
    fn test_resolve_base_commit_deepen_failure_propagates() {
        let dir = init_repo();
        make_orphan_branch(dir.path());
        let repo = GitRepo::open(dir.path()).unwrap();
        let event = GitEvent::PullRequest {
            base_ref: "main".to_string(),
            head_ref: "isolated".to_string(),
        };
        match resolve_base_commit(&repo, &event, ComparisonMode::MergeBase) {
            Err(Error::GitError(msg)) => assert!(msg.contains("deepen")),
            other => panic!("expected GitError about deepen fetch, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_default_branch_falls_back_to_main() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        assert_eq!(detect_default_branch(&repo).unwrap(), "main");
    }

    #[test]
    fn test_detect_default_branch_reads_symref_from_remote() {
        let dir = init_repo();
        let bare = tempfile::tempdir().unwrap();
        git(bare.path(), &["init", "--bare", "-b", "main"]);
        git(
            dir.path(),
            &["remote", "add", "origin", bare.path().to_str().unwrap()],
        );
        git(dir.path(), &["push", "origin", "main"]);

        let repo = GitRepo::open(dir.path()).unwrap();
        assert_eq!(detect_default_branch(&repo).unwrap(), "main");
    }

    #[test]
    fn test_get_origin_url_with_remote() {
        let dir = init_repo();
        let bare = tempfile::tempdir().unwrap();
        git(
            dir.path(),
            &["remote", "add", "origin", bare.path().to_str().unwrap()],
        );

        let repo = GitRepo::open(dir.path()).unwrap();
        let url = get_origin_url(&repo).unwrap();
        assert!(url.ends_with(".git") || !url.is_empty());
    }

    #[test]
    fn test_get_origin_url_without_remote_errors() {
        let dir = init_repo();
        let repo = GitRepo::open(dir.path()).unwrap();
        assert!(matches!(get_origin_url(&repo), Err(Error::GitError(_))));
    }

    #[test]
    fn test_git_event_accessors_use_explicit_revisions() {
        let push = GitEvent::Push {
            ref_name: "main".into(),
        };
        assert_eq!(push.base_ref(), "main");
        assert_eq!(push.head_ref(), "HEAD");

        let pr = GitEvent::PullRequest {
            base_ref: "base".into(),
            head_ref: "head".into(),
        };
        assert_eq!(pr.base_ref(), "base");
        assert_eq!(pr.head_ref(), "head");

        let merge_group = GitEvent::MergeGroup {
            base_ref: "queue-base".into(),
            head_ref: "queue-head".into(),
        };
        assert_eq!(merge_group.base_ref(), "queue-base");
        assert_eq!(merge_group.head_ref(), "queue-head");
    }

    #[test]
    #[serial]
    fn test_comparison_mode_from_env() {
        std::env::set_var("COMPARISON", "tip");
        assert!(matches!(ComparisonMode::from_env(), ComparisonMode::Tip));
        std::env::set_var("COMPARISON", "merge-base");
        assert!(matches!(
            ComparisonMode::from_env(),
            ComparisonMode::MergeBase
        ));
        std::env::remove_var("COMPARISON");
        assert!(matches!(
            ComparisonMode::from_env(),
            ComparisonMode::MergeBase
        ));
    }

    #[test]
    #[serial]
    fn test_is_fork_pr_true() {
        std::env::set_var("GITHUB_REPOSITORY", "owner/repo");
        std::env::set_var("GITHUB_HEAD_REPOSITORY", "fork/repo");
        assert!(is_fork_pr());
        std::env::remove_var("GITHUB_REPOSITORY");
        std::env::remove_var("GITHUB_HEAD_REPOSITORY");
    }

    #[test]
    #[serial]
    fn test_is_fork_pr_false() {
        std::env::set_var("GITHUB_REPOSITORY", "owner/repo");
        std::env::set_var("GITHUB_HEAD_REPOSITORY", "owner/repo");
        assert!(!is_fork_pr());
        std::env::remove_var("GITHUB_REPOSITORY");
        std::env::remove_var("GITHUB_HEAD_REPOSITORY");
    }

    #[test]
    #[serial]
    fn test_is_fork_pr_missing_vars() {
        std::env::remove_var("GITHUB_REPOSITORY");
        std::env::remove_var("GITHUB_HEAD_REPOSITORY");
        assert!(!is_fork_pr());
    }
}
