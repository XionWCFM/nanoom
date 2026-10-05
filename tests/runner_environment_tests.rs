use nanoom::prediction::{digest_value, RunnerEnvironment};
use std::process::Command;

fn collect(runner_name: &str) -> RunnerEnvironment {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.github/actions/run/environment.cjs"
        ))
        .env("RUNNER_NAME", runner_name)
        .env("GITHUB_RUN_ID", runner_name)
        .env(
            "INSTALL_RESULT",
            r#"{"packageManager":"pnpm","packageManagerVersion":"10.0.0"}"#,
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn native_collector_hash_matches_rust_and_excludes_ephemeral_identity() {
    let first = collect("runner-ephemeral-a");
    first.validate().unwrap();
    let second = collect("runner-ephemeral-b");
    assert_eq!(first, second);
    assert!(first.profile.available_cpus > 0);
    assert!(first.profile.memory_mi_b > 0);
    assert_eq!(first.profile.package_manager, "pnpm");
    assert_eq!(first.fingerprint, digest_value(&first.profile).unwrap());
}

#[test]
fn profile_changes_are_distinct_and_tampered_fingerprints_are_rejected() {
    let original = collect("runner");
    let mut profiles = Vec::new();
    let mut profile = original.profile.clone();
    profile.cpu_model.push_str("-other");
    profiles.push(profile);
    let mut profile = original.profile.clone();
    profile.available_cpus += 1;
    profiles.push(profile);
    let mut profile = original.profile.clone();
    profile.memory_mi_b += 1024;
    profiles.push(profile);
    let mut profile = original.profile.clone();
    profile.image.push_str("-new");
    profiles.push(profile);
    let mut profile = original.profile.clone();
    profile.cpu_quota_milli = Some(500);
    profiles.push(profile);
    let mut profile = original.profile.clone();
    profile.node_version.push_str("-new");
    profiles.push(profile);
    for profile in profiles {
        let mut environment = RunnerEnvironment {
            fingerprint: original.fingerprint.clone(),
            profile,
        };
        assert!(environment.validate().is_err());
        environment.fingerprint = digest_value(&environment.profile).unwrap();
        assert_ne!(environment.fingerprint, original.fingerprint);
        environment.validate().unwrap();
    }
    let mut invalid = original.clone();
    invalid.profile.available_cpus = 0;
    invalid.fingerprint = digest_value(&invalid.profile).unwrap();
    assert!(invalid.validate().is_err());
    let mut invalid = original;
    invalid.profile.image = "invalid\nimage".into();
    invalid.fingerprint = digest_value(&invalid.profile).unwrap();
    assert!(invalid.validate().is_err());
}

fn measurement(
    environment: Option<RunnerEnvironment>,
    execution: &str,
    duration: u64,
) -> nanoom::prediction::MeasurementArtifact {
    serde_json::from_value(serde_json::json!({
        "version": 3,
        "scope": {"repositoryKey":"github-12345","workflowPath":".github/workflows/ci.yml",
            "ref":{"kind":"push","ref":"refs/heads/main"},"group":"ci","taskRunner":"turbo","timingEnvironment":"shared-pool"},
        "runId":"123", "runAttempt":1,
        "runnerEnvironment": environment,
        "observations":[{"executionId":execution,"observedAtMs":20000 * nanoom::prediction::DAY_MS,
            "group":"ci","workspace":"app","task":"build","shard":null,"totalShards":null,
            "taskRunner":"turbo","timingEnvironment":"shared-pool","durationMs":duration}]
    })).unwrap()
}

#[test]
fn measured_profiles_survive_compile_learning_projection_and_exact_lookup() {
    use nanoom::prediction::{
        apply_batch, compile_measurements, project_predictions, ApplyOutcome, DAY_MS,
    };
    let fast = collect("fast");
    let mut slow = fast.clone();
    slow.profile.available_cpus = fast.profile.available_cpus + 1;
    slow.fingerprint = digest_value(&slow.profile).unwrap();
    let measurements = vec![
        measurement(Some(fast.clone()), "fast-1", 1_000),
        measurement(Some(fast.clone()), "fast-2", 1_000),
        measurement(Some(slow.clone()), "slow-1", 5_000),
        measurement(None, "legacy", 100_000),
    ];
    let at = 20000 * DAY_MS;
    let batch = compile_measurements(
        measurements[0].scope.clone(),
        "123".into(),
        1,
        at,
        &measurements,
    )
    .unwrap();
    assert_eq!(batch.environment_batches.len(), 2);
    let state = apply_batch(None, &batch, at).unwrap().0;
    assert_eq!(state.environment_states.len(), 2);
    let replay = apply_batch(Some(state.clone()), &batch, at).unwrap();
    assert_eq!(replay.1, ApplyOutcome::Duplicate);
    assert_eq!(replay.0, state);
    let table = project_predictions(&state, at).unwrap();
    table.validate().unwrap();
    let key = &batch.environment_batches[0].batch.aggregates[0].key_id;
    assert_eq!(
        table.estimate_for_environment(key, &fast.fingerprint, at),
        Some((1_000, 2))
    );
    assert_eq!(
        table.estimate_for_environment(key, &slow.fingerprint, at),
        Some((5_000, 1))
    );
    assert_eq!(
        table.estimate_for_environment(key, &"0".repeat(64), at),
        None
    );
    assert_eq!(table.estimate(key, at), Some((2_333, 3)));
    assert_eq!(table.environment_range(key, at), Some((2, 1_000, 5_000)));
    let context = nanoom::prediction::PredictionContext {
        repository_key: state.scope.repository_key.clone(),
        workflow_path: state.scope.workflow_path.clone(),
        git_ref: state.scope.git_ref.clone(),
    };
    let index = nanoom::prediction::PredictionIndex::from_tables([table.clone()]).unwrap();
    let item = nanoom::affected::WorkspaceEntry {
        group: "ci".into(),
        name: "app".into(),
        path: "packages/app".into(),
        task: "build".into(),
        shard: None,
        total_shards: None,
        checkout_paths: vec!["packages/app".into()],
    };
    let assignments = nanoom::scheduler::assign_with_prediction_index(
        "ci",
        &[item],
        1,
        &index,
        Some(&context),
        "turbo",
        "shared-pool",
        (None, None),
        at,
    );
    assert_eq!(assignments[0].predicted_duration_ms, 2_333);
    assert_eq!(assignments[0].prediction_sources.exact, 1);
    let uncertainty = assignments[0]
        .prediction_sources
        .environment_uncertainty
        .as_ref()
        .unwrap();
    assert_eq!(uncertainty.task_count, 1);
    assert_eq!(uncertainty.minimum_task_sum_ms, 1_000);
    assert_eq!(uncertainty.maximum_task_sum_ms, 5_000);
    let round_trip = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(project_predictions(&round_trip, at).unwrap(), table);
    let expired = project_predictions(&state, at + 30 * DAY_MS).unwrap();
    assert!(expired.rows.is_empty());
    assert!(expired.environment_predictions.is_empty());
    let mut nested = state.clone();
    nested.environment_states[0].state.environment_states = state.environment_states.clone();
    assert!(nanoom::prediction::validate_model(&nested).is_err());
    let mut wrong_scope = state;
    wrong_scope.environment_states[0].state.scope.group = "other".into();
    assert!(nanoom::prediction::validate_model(&wrong_scope).is_err());
}

#[test]
fn execution_identity_cannot_move_between_profiles_and_replay_body_is_immutable() {
    use nanoom::prediction::{apply_batch, compile_measurements, DAY_MS};
    let environment = collect("runner");
    let at = 20000 * DAY_MS;
    let classified = measurement(Some(environment), "same", 1_000);
    let legacy = measurement(None, "same", 1_000);
    assert!(compile_measurements(
        classified.scope.clone(),
        "123".into(),
        1,
        at,
        &[classified.clone(), legacy]
    )
    .is_err());
    let batch =
        compile_measurements(classified.scope.clone(), "123".into(), 1, at, &[classified]).unwrap();
    let state = apply_batch(None, &batch, at).unwrap().0;
    let mut changed = batch;
    changed.environment_batches[0].batch.aggregates[0].total_duration_ms += 1;
    assert!(apply_batch(Some(state), &changed, at)
        .unwrap_err()
        .contains("different body digest"));
}

#[test]
fn history_cli_publishes_environment_states_and_prediction_ranges() {
    use nanoom::prediction::{ModelStateBundle, PredictionArtifactBundle, DAY_MS};
    let directory = tempfile::tempdir().unwrap();
    let mut environments = [collect("one"), collect("two")];
    environments[1].profile.cpu_model.push_str("-other");
    environments[1].fingerprint = digest_value(&environments[1].profile).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_nanoom"));
    command.arg("history");
    for (index, environment) in environments.iter().enumerate() {
        let path = directory.path().join(format!("measurement-{index}.json"));
        std::fs::write(
            &path,
            serde_json::to_vec(&measurement(
                Some(environment.clone()),
                &index.to_string(),
                (index as u64 * 4 + 1) * 1_000,
            ))
            .unwrap(),
        )
        .unwrap();
        command.arg("--input").arg(path);
    }
    let model_path = directory.path().join("model.json");
    let prediction_path = directory.path().join("prediction.json");
    let output = command
        .arg("--model-output")
        .arg(&model_path)
        .arg("--prediction-output")
        .arg(&prediction_path)
        .args([
            "--model-artifact-name",
            "model-test",
            "--run-id",
            "123",
            "--run-attempt",
            "1",
            "--now-ms",
        ])
        .arg((20000 * DAY_MS).to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let model = ModelStateBundle::load(&model_path).unwrap();
    assert_eq!(model.states[0].environment_states.len(), 2);
    assert!(model.states[0].entries.is_empty());
    let predictions = PredictionArtifactBundle::load(&prediction_path).unwrap();
    let table = &predictions.predictions[0].table;
    assert_eq!(table.environment_predictions.len(), 2);
    let key = &table.rows[0].0;
    assert_eq!(table.estimate(key, 20000 * DAY_MS), Some((3_000, 2)));
    assert_eq!(
        table.environment_range(key, 20000 * DAY_MS),
        Some((2, 1_000, 5_000))
    );
}

#[test]
fn retired_image_does_not_dominate_the_next_pool_estimate() {
    use nanoom::prediction::{apply_batch, compile_measurements, project_predictions, DAY_MS};
    let original = collect("old-image");
    let mut replacement = original.clone();
    replacement.profile.image.push_str("-replacement");
    replacement.fingerprint = digest_value(&replacement.profile).unwrap();
    let at = 20000 * DAY_MS;
    let old: Vec<_> = (0..100)
        .map(|id| measurement(Some(original.clone()), &format!("old-{id}"), 1_000))
        .collect();
    let batch = compile_measurements(old[0].scope.clone(), "123".into(), 1, at, &old).unwrap();
    let state = apply_batch(None, &batch, at).unwrap().0;
    let mut current = measurement(Some(replacement.clone()), "replacement", 5_000);
    current.run_id = "124".into();
    current.observations[0].observed_at_ms = at + 10 * DAY_MS;
    let batch = compile_measurements(
        current.scope.clone(),
        "124".into(),
        1,
        at + 10 * DAY_MS,
        &[current],
    )
    .unwrap();
    let state = apply_batch(Some(state), &batch, at + 10 * DAY_MS)
        .unwrap()
        .0;
    let table = project_predictions(&state, at + 10 * DAY_MS).unwrap();
    let key = &batch.environment_batches[0].batch.aggregates[0].key_id;
    assert_eq!(table.estimate(key, at + 10 * DAY_MS), Some((5_000, 1)));
    assert_eq!(
        table.environment_range(key, at + 10 * DAY_MS),
        Some((1, 5_000, 5_000))
    );
    // Old history remains available when that particular machine is explicitly selected.
    assert_eq!(
        table.estimate_for_environment(key, &original.fingerprint, at + 10 * DAY_MS),
        Some((1_000, 100))
    );
}

#[test]
fn writing_another_profile_prunes_expired_keys_inside_a_still_live_profile() {
    use nanoom::prediction::{apply_batch, compile_measurements, project_predictions, DAY_MS};
    let first = collect("first");
    let mut second = first.clone();
    second.profile.cpu_model.push_str("-second");
    second.fingerprint = digest_value(&second.profile).unwrap();
    let at = 20000 * DAY_MS;
    let mut state = None;
    for (index, (day, environment, workspace)) in [
        (0, first.clone(), "expired"),
        (20, first.clone(), "live"),
        (31, second, "other"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut artifact = measurement(Some(environment), &format!("execution-{index}"), 1_000);
        artifact.run_id = (index + 1).to_string();
        artifact.observations[0].observed_at_ms = at + day * DAY_MS;
        artifact.observations[0].workspace = workspace.into();
        let batch = compile_measurements(
            artifact.scope.clone(),
            artifact.run_id.clone(),
            1,
            at + day * DAY_MS,
            &[artifact],
        )
        .unwrap();
        state = Some(apply_batch(state, &batch, at + day * DAY_MS).unwrap().0);
    }
    let state = state.unwrap();
    let child = &state
        .environment_states
        .iter()
        .find(|child| child.environment.fingerprint == first.fingerprint)
        .unwrap()
        .state;
    assert_eq!(child.entries.len(), 2); // Live workspace exact + build fallback.
    assert!(child
        .entries
        .iter()
        .all(|entry| entry.buckets.len() == 1 && entry.buckets[0].0 == 20020));
    assert!(child.receipts.is_empty());
    nanoom::prediction::validate_model(&state).unwrap();
    project_predictions(&state, at + 31 * DAY_MS)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn profile_partitioning_is_order_independent_and_deduplicates_measurement_replay() {
    use nanoom::prediction::{
        apply_batch, canonical_bytes, compile_measurements, project_predictions, DAY_MS,
    };
    let one = collect("one");
    let mut two = one.clone();
    two.profile.image.push_str("-two");
    two.fingerprint = digest_value(&two.profile).unwrap();
    let at = 20000 * DAY_MS;
    let artifacts = [
        measurement(Some(one), "one", 1_000),
        measurement(Some(two), "two", 5_000),
    ];
    let scope = artifacts[0].scope.clone();
    let forward = compile_measurements(scope.clone(), "123".into(), 1, at, &artifacts).unwrap();
    let repeated = compile_measurements(
        scope,
        "123".into(),
        1,
        at,
        &[
            artifacts[1].clone(),
            artifacts[0].clone(),
            artifacts[1].clone(),
        ],
    )
    .unwrap();
    assert_eq!(
        canonical_bytes(&forward).unwrap(),
        canonical_bytes(&repeated).unwrap()
    );
    let state = apply_batch(None, &forward, at).unwrap().0;
    let table = project_predictions(&state, at).unwrap();
    assert!(table.rows.iter().all(|row| row.1 == 3_000 && row.2 == 2));
    let mut wrong_scope = forward.clone();
    wrong_scope.environment_batches[0].batch.scope.group = "unauthorized".into();
    assert!(apply_batch(None, &wrong_scope, at).is_err());
    let mut nested = forward.clone();
    nested.environment_batches[0].batch.environment_batches = forward.environment_batches.clone();
    assert!(apply_batch(None, &nested, at).is_err());
    let mut reordered = forward;
    reordered.environment_batches.reverse();
    assert!(apply_batch(None, &reordered, at).is_err());
    let mut nested = table.clone();
    nested.environment_predictions[0]
        .table
        .environment_predictions = table.environment_predictions.clone();
    assert!(nested.validate().is_err());
    let mut duplicate = table.clone();
    duplicate
        .environment_predictions
        .push(table.environment_predictions[0].clone());
    assert!(duplicate.validate().is_err());
    let mut tampered = table;
    tampered.environment_predictions[0]
        .environment
        .profile
        .memory_mi_b += 1;
    assert!(tampered.validate().is_err());
}

#[test]
fn native_collector_honors_nested_and_inherited_cgroup_limits() {
    let script = r#"
const assert = require('node:assert/strict');
const {cgroupLimits} = require(process.argv[1]);
const files = {
  '/proc/self/cgroup': '0::/runner/job',
  '/sys/fs/cgroup/cpu.max': 'max 100000',
  '/sys/fs/cgroup/memory.max': 'max',
  '/sys/fs/cgroup/runner/cpu.max': '100000 100000',
  '/sys/fs/cgroup/runner/memory.max': '1073741824',
  '/sys/fs/cgroup/runner/job/cpu.max': '200000 100000',
  '/sys/fs/cgroup/runner/job/memory.max': '536870912',
};
const read = path => files[path] ?? null;
assert.deepEqual(cgroupLimits(read), {cpuQuotaMilli:1000,memoryLimitMiB:512,containerLimits:'cgroup-v2'});
files['/sys/fs/cgroup/runner/job/cpu.max'] = '50000 100000';
assert.equal(cgroupLimits(read).cpuQuotaMilli, 500);
files['/proc/self/cgroup'] = '0::/../../outside';
assert.deepEqual(cgroupLimits(read), {cpuQuotaMilli:null,memoryLimitMiB:null,containerLimits:'cgroup-v2'});
files['/proc/self/cgroup'] = '2:cpu:/runner/job';
assert.deepEqual(cgroupLimits(read), {cpuQuotaMilli:null,memoryLimitMiB:null,containerLimits:'unknown'});
assert.deepEqual(cgroupLimits(() => null), {cpuQuotaMilli:null,memoryLimitMiB:null,containerLimits:'unknown'});
"#;
    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.github/actions/run/environment.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ten_thousand_workspaces_in_two_profiles_fit_bounded_artifact_contracts() {
    use nanoom::prediction::{
        apply_batch, canonical_bytes, compile_measurements, project_predictions, DAY_MS,
    };
    let first = collect("first");
    let mut second = first.clone();
    second.profile.cpu_model.push_str("-second");
    second.fingerprint = digest_value(&second.profile).unwrap();
    let mut measurements = Vec::new();
    for (profile_index, environment) in [first, second].into_iter().enumerate() {
        let mut artifact = measurement(Some(environment), "seed", 1_000);
        let seed = artifact.observations[0].clone();
        artifact.observations = (0..10_000)
            .map(|index| {
                let mut row = seed.clone();
                row.execution_id = format!("profile-{profile_index}-workspace-{index}");
                row.workspace = format!("workspace-{index}");
                row.duration_ms =
                    (if index % 2 == 0 { 1_000 } else { 5_000 }) * (profile_index as u64 + 1);
                row
            })
            .collect();
        measurements.push(artifact);
    }
    let at = 20000 * DAY_MS;
    let batch = compile_measurements(
        measurements[0].scope.clone(),
        "123".into(),
        1,
        at,
        &measurements,
    )
    .unwrap();
    let state = apply_batch(None, &batch, at).unwrap().0;
    let table = project_predictions(&state, at).unwrap();
    table.validate().unwrap();
    assert_eq!(state.environment_states.len(), 2);
    assert_eq!(table.rows.len(), 10_001);
    assert_eq!(
        table
            .environment_predictions
            .iter()
            .map(|child| child.table.rows.len())
            .sum::<usize>(),
        20_002
    );
    let model_bytes = canonical_bytes(&state).unwrap().len();
    let prediction_bytes = canonical_bytes(&table).unwrap().len();
    assert!(model_bytes < 16 * 1024 * 1024);
    assert!(prediction_bytes < 8 * 1024 * 1024);
    let workspace_key = nanoom::prediction::PredictionKey::TaskExact {
        group: "ci".into(),
        workspace: "workspace-0".into(),
        task: "build".into(),
        shard: None,
        total_shards: None,
        task_runner: "turbo".into(),
        timing_environment: "shared-pool".into(),
    }
    .id()
    .unwrap();
    assert_eq!(table.estimate(&workspace_key, at), Some((1_500, 2)));
    assert_eq!(
        table.environment_range(&workspace_key, at),
        Some((2, 1_000, 2_000))
    );
    eprintln!("10,000 workspaces / 2 profiles: model={model_bytes} bytes, prediction={prediction_bytes} bytes");
}

#[test]
fn fresh_unclassified_measurement_replaces_profiles_outside_the_recent_window() {
    use nanoom::prediction::{apply_batch, compile_measurements, project_predictions, DAY_MS};
    let at = 20000 * DAY_MS;
    let initial = measurement(Some(collect("runner")), "old", 1_000);
    let batch =
        compile_measurements(initial.scope.clone(), "123".into(), 1, at, &[initial]).unwrap();
    let state = apply_batch(None, &batch, at).unwrap().0;
    let mut current = measurement(None, "missing-telemetry", 5_000);
    current.run_id = "124".into();
    current.observations[0].observed_at_ms = at + 10 * DAY_MS;
    let batch = compile_measurements(
        current.scope.clone(),
        "124".into(),
        1,
        at + 10 * DAY_MS,
        &[current],
    )
    .unwrap();
    let state = apply_batch(Some(state), &batch, at + 10 * DAY_MS)
        .unwrap()
        .0;
    let table = project_predictions(&state, at + 10 * DAY_MS).unwrap();
    let key = &batch.aggregates[0].key_id;
    assert_eq!(table.estimate(key, at + 10 * DAY_MS), Some((5_000, 1)));
    assert_eq!(table.environment_range(key, at + 10 * DAY_MS), None);
}
