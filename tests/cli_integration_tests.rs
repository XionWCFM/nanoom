use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn write_json(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn init_git_repo(path: &Path) {
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
        vec!["add", "."],
        vec!["commit", "-m", "init", "--no-gpg-sign"],
        vec!["branch", "-M", "main"],
    ] {
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(&args)
            .output()
            .unwrap();
    }
}

/// Builds a monorepo fixture: root package.json with yarn workspaces,
/// two packages, and a nanoom.config.json with one group.
fn setup_monorepo(dir: &Path) {
    write_json(
        &dir.join("package.json"),
        &serde_json::json!({
            "name": "root",
            "private": true,
            "workspaces": ["packages/*"]
        }),
    );

    for name in ["pkg-a", "pkg-b"] {
        write_json(
            &dir.join(format!("packages/{}/package.json", name)),
            &serde_json::json!({
                "name": name,
                "version": "1.0.0",
                "scripts": { "test": "exit 0" }
            }),
        );
    }

    write_json(
        &dir.join("nanoom.config.json"),
        &serde_json::json!({
            "group": {
                "ci": { "tasks": ["test"] }
            },
            "globalDependencies": ["*.lock"]
        }),
    );

    fs::write(dir.join("pnpm-lock.yaml"), "").unwrap_or(());
    let _ = fs::remove_file(dir.join("pnpm-lock.yaml"));
}

fn binary_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // deps/
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("nanoom")
}

fn run_cli(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (bool, String) {
    let (success, stdout, stderr) = run_cli_parts(cwd, args, envs);
    (success, format!("{}{}", stdout, stderr))
}

fn run_cli_parts(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = Command::new(binary_path());
    cmd.current_dir(cwd).args(args);

    // Ensure no CI env leaks from parent process

    for (key, value) in envs {
        cmd.env(key, value);
    }

    let output = cmd.output().expect("failed to run nanoom binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

#[test]
fn json_mode_keeps_stdout_machine_readable_and_diagnostics_on_stderr() {
    let dir = tempdir().unwrap();
    let (success, stdout, stderr) = run_cli_parts(dir.path(), &["version", "--json"], &[]);
    assert!(success);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["name"], "nanoom");
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
}

#[test]
fn json_mode_returns_json_error_and_nonzero_exit() {
    let dir = tempdir().unwrap();
    let (success, stdout, stderr) = run_cli_parts(dir.path(), &["affected", "--json"], &[]);
    assert!(!success);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["status"], "failure");
    assert!(value["error"].as_str().is_some());
    assert!(stderr.contains("error:"));
}

#[test]
fn cache_key_json_is_a_single_json_object() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("nanoom.config.json"),
        "{\"group\":{\"ci\":{\"tasks\":[\"test\"]}}}\n",
    )
    .unwrap();
    let (success, stdout, stderr) = run_cli_parts(
        dir.path(),
        &["cache-key", "--runner", "turbo", "--task", "test", "--json"],
        &[],
    );
    assert!(success);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert!(value["key"]
        .as_str()
        .unwrap()
        .starts_with("nanoom-turbo-test-"));
    assert!(stderr.is_empty());
}

#[test]
fn test_cli_help_exits_successfully() {
    let dir = tempdir().unwrap();
    let (success, output) = run_cli(dir.path(), &["--help"], &[]);
    assert!(success);
    assert!(output.contains("affected"));
    assert!(output.contains("run"));
    assert!(output.contains("install"));
    assert!(output.contains("status"));
    assert!(output.contains("schema"));
}

#[test]
fn test_cli_version() {
    let dir = tempdir().unwrap();
    let (success, output) = run_cli(dir.path(), &["--version"], &[]);
    assert!(success);
    assert!(output.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_schema_command_outputs_valid_json() {
    let dir = tempdir().unwrap();
    let (success, output) = run_cli(dir.path(), &["schema"], &[]);
    assert!(success);

    let schema: serde_json::Value = serde_json::from_str(output.trim())
        .unwrap_or_else(|e| panic!("schema is not valid JSON: {} ({})", e, output));
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
}

#[test]
fn test_schema_to_file() {
    let dir = tempdir().unwrap();
    let out_path = dir.path().join("out.schema.json");
    let out_str = out_path.to_str().unwrap();

    let (success, _) = run_cli(dir.path(), &["schema", "--output", out_str], &[]);
    assert!(success);
    assert!(out_path.exists());

    let content = fs::read_to_string(&out_path).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(schema["properties"].is_object());
}

#[test]
fn test_affected_requires_config() {
    let dir = tempdir().unwrap();
    let (_, output) = run_cli(dir.path(), &["affected"], &[]);
    assert!(
        output.contains("not found") || output.contains("Config"),
        "unexpected output: {}",
        output
    );
}

#[test]
fn test_affected_requires_explicit_base() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    init_git_repo(dir.path());

    let (success, output) = run_cli(dir.path(), &["affected", "--json"], &[]);
    assert!(!success);
    assert!(
        output.contains("requires --base"),
        "unexpected output: {output}"
    );
}

#[test]
fn test_affected_pull_request_event_with_changes() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    init_git_repo(dir.path());

    // Simulate a PR: commit changes on a feature branch after base exists on main.
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["checkout", "-b", "feature"])
        .output()
        .unwrap();

    fs::write(dir.path().join("packages/pkg-a/new-file.ts"), "export {}").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["commit", "-m", "change pkg-a", "--no-gpg-sign"])
        .output()
        .unwrap();

    let (success, output) = run_cli(
        dir.path(),
        &["affected", "--json", "--base", "main", "--head", "feature"],
        &[],
    );

    assert!(success, "command failed: {}", output);

    let parsed: serde_json::Value =
        serde_json::from_str(output.trim()).expect("invalid JSON output");

    assert_eq!(parsed["has_change"], true);
    let workspaces = parsed["group"]["ci"]["workspaces"]
        .as_array()
        .expect("ci group missing");
    let names: Vec<&str> = workspaces
        .iter()
        .filter_map(|e| e["name"].as_str())
        .collect();
    assert!(
        names.contains(&"pkg-a"),
        "pkg-a should be affected: {:?}",
        names
    );
}

#[test]
fn test_affected_no_changes_between_same_ref() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    init_git_repo(dir.path());

    let (success, output) = run_cli(
        dir.path(),
        &["affected", "--base", "main", "--head", "main"],
        &[],
    );
    assert!(success, "command failed: {}", output);
    assert!(output.contains("Has changes: false") || output.contains("\"has_change\":false"));
}

#[test]
fn test_affected_matrix_output() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    init_git_repo(dir.path());

    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["checkout", "-b", "feature"])
        .output()
        .unwrap();
    fs::write(dir.path().join("packages/pkg-b/changed.ts"), "x").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["commit", "-m", "change pkg-b", "--no-gpg-sign"])
        .output()
        .unwrap();

    let (success, output) = run_cli(
        dir.path(),
        &[
            "affected", "--matrix", "ci", "--base", "main", "--head", "feature",
        ],
        &[],
    );
    assert!(success, "command failed: {}", output);

    let matrix: serde_json::Value =
        serde_json::from_str(output.trim()).expect("invalid matrix JSON");
    let include = matrix["include"]
        .as_array()
        .expect("matrix include missing");

    let names: Vec<&str> = include.iter().filter_map(|e| e["name"].as_str()).collect();
    assert!(names.contains(&"pkg-b"));
}

#[test]
fn test_affected_push_event_unknown_branch_fails_gracefully() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    init_git_repo(dir.path());

    let (success, _output) = run_cli(
        dir.path(),
        &["affected", "--base", "nonexistent-branch"],
        &[],
    );
    assert!(!success, "unknown branch should produce an error");
}

#[test]
fn test_run_unknown_group_fails() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    init_git_repo(dir.path());

    let (success, output) = run_cli(dir.path(), &["run", "no-such-group", "test"], &[]);
    assert!(!success);
    assert!(output.contains("not found"));
}

fn minimal_config(dir: &Path) {
    write_json(
        &dir.join("nanoom.config.json"),
        &serde_json::json!({
            "group": { "ci": { "tasks": ["test"] } }
        }),
    );
}

#[test]
fn test_status_aggregates_from_github_output() {
    let dir = tempdir().unwrap();
    minimal_config(dir.path());

    let gh_output = dir.path().join("github_output.txt");
    fs::write(&gh_output, "ci_result=success\ne2e_result=failure\n").unwrap();

    let envs = [("GITHUB_OUTPUT", gh_output.to_str().unwrap())];
    let (success, output) = run_cli(dir.path(), &["status", "ci,e2e", "--format", "json"], &envs);

    // e2e failed -> overall failure and non-zero exit
    assert!(
        !success,
        "should exit non-zero when any job failed: {}",
        output
    );
    let parsed: serde_json::Value = serde_json::from_str(output.trim()).expect("invalid JSON");
    assert_eq!(parsed["overall"], "failure");

    let jobs = parsed["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 2);
}

#[test]
fn test_status_all_success_exits_zero() {
    let dir = tempdir().unwrap();
    minimal_config(dir.path());

    let gh_output = dir.path().join("github_output.txt");
    fs::write(&gh_output, "ci_result=success\n").unwrap();

    let envs = [("GITHUB_OUTPUT", gh_output.to_str().unwrap())];
    let (success, _output) = run_cli(dir.path(), &["status", "ci", "--format", "text"], &envs);
    assert!(success);
}

#[test]
fn test_affected_base_flag_overrides_missing_env() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    init_git_repo(dir.path());

    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["checkout", "-b", "feature"])
        .output()
        .unwrap();
    fs::write(dir.path().join("packages/pkg-a/change.ts"), "x").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["commit", "-m", "change pkg-a", "--no-gpg-sign"])
        .output()
        .unwrap();

    let (success, output) = run_cli(dir.path(), &["affected", "--base", "main"], &[]);
    assert!(
        success,
        "--base must work without event env vars: {}",
        output
    );
    assert!(output.contains("Has changes: true") || output.contains("\"has_change\":true"));
}

#[test]
fn test_global_cwd_flag_works_from_outside_repo() {
    let repo = tempdir().unwrap();
    setup_monorepo(repo.path());
    init_git_repo(repo.path());

    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["checkout", "-b", "feature"])
        .output()
        .unwrap();
    fs::write(repo.path().join("packages/pkg-a/change.ts"), "x").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["commit", "-m", "change pkg-a", "--no-gpg-sign"])
        .output()
        .unwrap();

    let elsewhere = tempdir().unwrap();
    let mut cmd = Command::new(binary_path());
    cmd.current_dir(elsewhere.path())
        .arg("-C")
        .arg(repo.path())
        .args(["affected", "--json", "--base", "main", "--head", "feature"]);
    let output = cmd.output().expect("failed to run nanoom binary");

    assert!(
        output.status.success(),
        "global -C should drive command execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
        .expect("invalid JSON output");
    assert_eq!(parsed["has_change"], true);
}

#[test]
fn test_affected_preserves_all_entries_when_exceeding_concurrency() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_json(
        &root.join("package.json"),
        &serde_json::json!({
            "name": "root",
            "private": true,
            "workspaces": ["packages/*"]
        }),
    );

    for name in ["pkg-a", "pkg-b", "pkg-c"] {
        write_json(
            &root.join(format!("packages/{}/package.json", name)),
            &serde_json::json!({
                "name": name,
                "version": "1.0.0",
                "scripts": { "test": "exit 0" }
            }),
        );
    }

    write_json(
        &root.join("nanoom.config.json"),
        &serde_json::json!({
            "group": {
                "ci": {
                    "tasks": ["test"],
                    "rules": [
                        { "name": "pkg-a", "shard": [{ "task": "test", "shard": 2 }] }
                    ]
                }
            }
        }),
    );

    init_git_repo(dir.path());

    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["checkout", "-b", "feature"])
        .output()
        .unwrap();
    for name in ["pkg-a", "pkg-b", "pkg-c"] {
        fs::write(root.join(format!("packages/{}/change.ts", name)), "x").unwrap();
    }
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["commit", "-m", "touch all packages", "--no-gpg-sign"])
        .output()
        .unwrap();

    let (success, output) = run_cli(
        dir.path(),
        &["affected", "--json", "--base", "main", "--head", "feature"],
        &[],
    );
    assert!(success, "command failed: {}", output);

    let parsed: serde_json::Value =
        serde_json::from_str(output.trim()).expect("invalid JSON output");
    let workspaces = parsed["group"]["ci"]["workspaces"]
        .as_array()
        .expect("ci group missing");

    assert_eq!(
        workspaces.len(),
        4,
        "all entries must survive regardless of concurrency: {:?}",
        workspaces
    );

    for entry in workspaces {
        assert_eq!(
            entry["task"], "test",
            "task names must not be rewritten with batch labels: {:?}",
            entry
        );
    }

    let pkg_a_shards: Vec<i64> = workspaces
        .iter()
        .filter(|e| e["name"] == "pkg-a")
        .filter_map(|e| e["shard"].as_i64())
        .collect();
    assert_eq!(
        pkg_a_shards,
        vec![1, 2],
        "shard metadata must be preserved: {:?}",
        workspaces
    );
}

#[test]
fn test_run_executes_affected_workspace_script_end_to_end() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    // Make the executed command observable while keeping it dependency-free.
    write_json(
        &dir.path().join("packages/pkg-a/package.json"),
        &serde_json::json!({
            "name": "pkg-a",
            "version": "1.0.0",
            "scripts": { "test": "node -e \"console.log('pkg-a-e2e')\"" }
        }),
    );
    init_git_repo(dir.path());
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["checkout", "-b", "feature"])
        .output()
        .unwrap();
    fs::write(dir.path().join("packages/pkg-a/changed.ts"), "x").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["commit", "-m", "change", "--no-gpg-sign"])
        .output()
        .unwrap();

    let (success, output) = run_cli(dir.path(), &["run", "ci", "test", "--all"], &[]);
    assert!(success, "run failed: {}", output);
    assert!(
        output.contains("pkg-a-e2e"),
        "script did not execute: {}",
        output
    );
}

#[test]
fn test_install_handles_root_monorepo_without_workspace_lockfiles() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    let (success, output) = run_cli(dir.path(), &["install"], &[]);
    assert!(success, "install failed: {}", output);
    assert!(dir.path().join("package-lock.json").exists());
}

#[test]
fn test_run_shards_execute_distinct_shard_contexts_end_to_end() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    write_json(
        &dir.path().join("nanoom.config.json"),
        &serde_json::json!({
            "group": {
                "ci": {
                    "tasks": ["test"],
                    "rules": [{
                        "name": "pkg-a",
                        "shard": [{ "task": "test", "shard": 2 }]
                    }]
                }
            }
        }),
    );
    write_json(
        &dir.path().join("packages/pkg-a/package.json"),
        &serde_json::json!({
            "name": "pkg-a",
            "version": "1.0.0",
            "scripts": { "test": "node -e \"console.log(process.env.NANOOM_SHARD_INDEX + '/' + process.env.NANOOM_SHARD_TOTAL)\"" }
        }),
    );

    let (first_ok, first_output) = run_cli(
        dir.path(),
        &[
            "run",
            "ci",
            "test",
            "--all",
            "--shard",
            "1",
            "--total-shards",
            "2",
        ],
        &[],
    );
    let (second_ok, second_output) = run_cli(
        dir.path(),
        &[
            "run",
            "ci",
            "test",
            "--all",
            "--shard",
            "2",
            "--total-shards",
            "2",
        ],
        &[],
    );
    assert!(first_ok, "first shard failed: {}", first_output);
    assert!(second_ok, "second shard failed: {}", second_output);
    assert!(first_output.contains("1/2"));
    assert!(second_output.contains("2/2"));
    assert!(!first_output.contains("pkg-b"));
    assert!(!second_output.contains("pkg-b"));
}

#[test]
fn test_run_isolate_selects_only_isolated_workspace_end_to_end() {
    let dir = tempdir().unwrap();
    setup_monorepo(dir.path());
    write_json(
        &dir.path().join("nanoom.config.json"),
        &serde_json::json!({
            "group": {
                "ci": {
                    "tasks": ["test"],
                    "rules": [{ "name": "pkg-a", "isolate": ["test"] }]
                }
            }
        }),
    );
    write_json(
        &dir.path().join("packages/pkg-a/package.json"),
        &serde_json::json!({
            "name": "pkg-a", "version": "1.0.0",
            "scripts": { "test": "node -e \"console.log('isolated-a')\"" }
        }),
    );
    write_json(
        &dir.path().join("packages/pkg-b/package.json"),
        &serde_json::json!({
            "name": "pkg-b", "version": "1.0.0",
            "scripts": { "test": "node -e \"console.log('regular-b')\"" }
        }),
    );
    let (success, output) = run_cli(
        dir.path(),
        &["run", "ci", "test", "--all", "--isolate"],
        &[],
    );
    assert!(success, "isolate run failed: {}", output);
    assert!(output.contains("isolated-a"));
    assert!(!output.contains("regular-b"));
}
