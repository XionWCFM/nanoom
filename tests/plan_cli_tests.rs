use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn init_repo(dir: &Path) {
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
        vec!["add", "."],
        vec!["commit", "-m", "base", "--no-gpg-sign"],
        vec!["branch", "-M", "main"],
    ] {
        git(dir, &args);
    }
}

fn run_cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nanoom"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

struct Fixture {
    dir: tempfile::TempDir,
    base: String,
    head: String,
    plan: PathBuf,
    reference: PathBuf,
}

fn changed_fixture() -> Fixture {
    let dir = tempdir().unwrap();
    write_json(
        &dir.path().join("package.json"),
        &json!({"name":"root","private":true,"workspaces":["packages/*"]}),
    );
    write_json(
        &dir.path().join("packages/pkg-a/package.json"),
        &json!({"name":"pkg-a","version":"1.0.0","scripts":{"test":"exit 0"}}),
    );
    write_json(
        &dir.path().join("nanoom.config.json"),
        &json!({"group":{"ci":{"tasks":["test"]}}}),
    );
    init_repo(dir.path());
    let base = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["checkout", "-b", "feature"]);
    fs::write(dir.path().join("packages/pkg-a/change.txt"), "change\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "change", "--no-gpg-sign"]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let plan = dir.path().join("plan.json");
    let reference = dir.path().join("reference.json");
    let context = json!({
        "repository":"owner/repo",
        "workflow":".github/workflows/ci.yml@refs/heads/main",
        "runId":"12345",
        "producerAttempt":1,
        "planningJob":"affected",
        "base":base,
        "head":head,
        "taskRunner":"pnpm",
        "predictionReason":"cold start"
    });
    write_json(&dir.path().join("context.json"), &context);
    let output = run_cli(
        dir.path(),
        &[
            "-C",
            ".",
            "affected",
            "--base",
            "main",
            "--head",
            "feature",
            "--timing-runner",
            "pnpm",
            "--plan-output",
            "plan.json",
            "--plan-context",
            "context.json",
        ],
    );
    assert!(
        output.status.success(),
        "affected failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compact: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(compact["has_change"], true);
    assert_eq!(compact["result"]["itemCount"], 1);
    assert_eq!(compact["result"]["historyStatus"], "disabled");
    assert_eq!(compact["result"]["timingRunner"], "pnpm");
    assert_eq!(
        compact["groups"]["ci"]["include"][0],
        json!({"group":"ci","assignmentId":"ci-0001"})
    );
    write_json(&reference, &compact["plan"]);
    Fixture {
        dir,
        base,
        head,
        plan,
        reference,
    }
}

fn select(fixture: &Fixture, output_dir: &str, assignment: &str) -> Output {
    run_cli(
        fixture.dir.path(),
        &[
            "plan",
            "select",
            "--input",
            "plan.json",
            "--reference",
            "reference.json",
            "--group",
            "ci",
            "--assignment",
            assignment,
            "--output-dir",
            output_dir,
        ],
    )
}

#[test]
fn affected_writes_plan_and_selects_validated_assignment_files() {
    let fixture = changed_fixture();
    let reference: Value = serde_json::from_slice(&fs::read(&fixture.reference).unwrap()).unwrap();
    assert_eq!(reference["version"], 1);
    let plan: Value = serde_json::from_slice(&fs::read(&fixture.plan).unwrap()).unwrap();
    let expected_digest = format!("{:x}", Sha256::digest(fs::read(&fixture.plan).unwrap()));
    assert_eq!(reference["sha256"], expected_digest);
    assert_eq!(plan["version"], 1);
    assert_eq!(plan["provenance"]["base"], fixture.base);
    assert_eq!(plan["provenance"]["head"], fixture.head);
    assert_eq!(
        plan["groups"]["ci"]["assignments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    fs::remove_file(fixture.dir.path().join("nanoom.config.json")).unwrap();
    let output = select(&fixture, "selected", "ci-0001");
    assert!(
        output.status.success(),
        "plan select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assignment: Value = serde_json::from_slice(
        &fs::read(fixture.dir.path().join("selected/assignment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(assignment["items"][0]["name"], "pkg-a");
    assert_eq!(
        fs::read_to_string(fixture.dir.path().join("selected/paths.txt")).unwrap(),
        "packages/pkg-a\n"
    );
}

#[test]
fn plan_select_reuses_same_run_prior_attempt_and_rejects_other_run_or_head() {
    let fixture = changed_fixture();
    let mut reference: Value =
        serde_json::from_slice(&fs::read(&fixture.reference).unwrap()).unwrap();
    reference["current"]["attempt"] = json!(2);
    write_json(&fixture.reference, &reference);
    assert!(select(&fixture, "retry", "ci-0001").status.success());

    reference["current"]["runId"] = json!("99999");
    write_json(&fixture.reference, &reference);
    let output = select(&fixture, "wrong-run", "ci-0001");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("different repository"));

    reference["current"]["runId"] = json!("12345");
    reference["current"]["head"] = json!("c".repeat(40));
    write_json(&fixture.reference, &reference);
    let output = select(&fixture, "wrong-head", "ci-0001");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("different repository"));
}

#[test]
fn plan_select_rejects_corrupt_digest_tampered_plan_and_missing_assignment() {
    let fixture = changed_fixture();
    let original_plan = fs::read(&fixture.plan).unwrap();
    let original_reference = fs::read(&fixture.reference).unwrap();
    let mut reference: Value = serde_json::from_slice(&original_reference).unwrap();
    reference["sha256"] = json!("0".repeat(64));
    write_json(&fixture.reference, &reference);
    let output = select(&fixture, "bad-reference", "ci-0001");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SHA-256"));
    assert!(!fixture.dir.path().join("bad-reference").exists());
    fs::write(&fixture.plan, &original_plan).unwrap();

    let mut plan: Value = serde_json::from_slice(&original_plan).unwrap();
    plan["groups"]["ci"]["assignments"][0]["items"][0]["task"] = json!("build");
    write_json(&fixture.plan, &plan);
    let output = select(&fixture, "tampered-plan", "ci-0001");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SHA-256"));
    assert!(!fixture.dir.path().join("tampered-plan").exists());
    fs::write(&fixture.plan, &original_plan).unwrap();

    fs::write(&fixture.reference, &original_reference).unwrap();
    let output = select(&fixture, "missing-assignment", "ci-9999");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not exist"),
        "unexpected selection error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.dir.path().join("missing-assignment").exists());
}

#[test]
fn affected_writes_a_valid_zero_work_plan() {
    let fixture = changed_fixture();
    let context = json!({
        "repository":"owner/repo",
        "workflow":".github/workflows/ci.yml@refs/heads/main",
        "runId":"12345",
        "producerAttempt":1,
        "planningJob":"affected",
        "base":fixture.head,
        "head":fixture.head,
        "taskRunner":"pnpm",
        "predictionReason":"no changes"
    });
    write_json(&fixture.dir.path().join("zero-context.json"), &context);
    let output = run_cli(
        fixture.dir.path(),
        &[
            "affected",
            "--base",
            &fixture.head,
            "--head",
            &fixture.head,
            "--timing-runner",
            "pnpm",
            "--plan-output",
            "zero-plan.json",
            "--plan-context",
            "zero-context.json",
        ],
    );
    assert!(
        output.status.success(),
        "zero work failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compact: Value = serde_json::from_slice(&output.stdout).unwrap();
    let plan: Value =
        serde_json::from_slice(&fs::read(fixture.dir.path().join("zero-plan.json")).unwrap())
            .unwrap();
    assert_eq!(compact["has_change"], false);
    assert_eq!(compact["result"]["assignmentCount"], 0);
    assert_eq!(plan["itemCount"], 0);
    assert_eq!(plan["assignmentCount"], 0);
    assert!(plan["groups"]["ci"]["assignments"]
        .as_array()
        .unwrap()
        .is_empty());
    write_json(&fixture.reference, &compact["plan"]);
    let output = run_cli(
        fixture.dir.path(),
        &[
            "plan",
            "select",
            "--input",
            "zero-plan.json",
            "--reference",
            "reference.json",
            "--group",
            "ci",
            "--assignment",
            "ci-0001",
            "--output-dir",
            "zero-selected",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not exist"));
    assert!(!fixture.dir.path().join("zero-selected").exists());
}

#[test]
fn affected_rejects_unbounded_json_when_plan_output_is_requested() {
    let fixture = changed_fixture();
    let output = run_cli(
        fixture.dir.path(),
        &[
            "affected",
            "--json",
            "--base",
            "main",
            "--head",
            "feature",
            "--plan-output",
            "combined.json",
            "--plan-context",
            "context.json",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be combined"));
    assert!(!fixture.dir.path().join("combined.json").exists());
}

#[test]
fn affected_rejects_plan_context_for_a_different_execution_tool() {
    let fixture = changed_fixture();
    let mut context: Value =
        serde_json::from_slice(&fs::read(fixture.dir.path().join("context.json")).unwrap())
            .unwrap();
    context["taskRunner"] = json!("yarn");
    write_json(
        &fixture.dir.path().join("wrong-tool-context.json"),
        &context,
    );
    let output = run_cli(
        fixture.dir.path(),
        &[
            "affected",
            "--base",
            "main",
            "--head",
            "feature",
            "--timing-runner",
            "pnpm",
            "--plan-output",
            "wrong-tool-plan.json",
            "--plan-context",
            "wrong-tool-context.json",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not match resolved affected runner")
    );
    assert!(!fixture.dir.path().join("wrong-tool-plan.json").exists());
}
