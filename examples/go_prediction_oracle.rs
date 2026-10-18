//! Regenerate the Go port's independent compatibility corpus:
//! cargo run --locked --example go_prediction_oracle > internal/prediction/testdata/rust-v3.json
use nanoom_prediction_core::*;
use serde_json::json;

fn scope() -> Scope {
    Scope {
        repository_key: "github-12345".into(),
        workflow_path: ".github/workflows/ci.yml".into(),
        git_ref: ScopeRef::Push {
            git_ref: "refs/heads/main".into(),
        },
        group: "ci".into(),
        task_runner: "yarn".into(),
        timing_environment: "linux-x64-node24".into(),
    }
}
fn observation(id: &str, at: u64, duration: u64) -> SuccessfulObservation {
    SuccessfulObservation {
        execution_id: id.into(),
        observed_at_ms: at,
        group: "ci".into(),
        workspace: "@fixture/web".into(),
        task: "test".into(),
        shard: None,
        total_shards: None,
        task_runner: "yarn".into(),
        timing_environment: "linux-x64-node24".into(),
        duration_ms: duration,
    }
}
fn main() {
    let start = 1_728_000_000_000_u64;
    let mut scenarios = Vec::new();
    for with_environment in [false, true] {
        let mut prior: Option<ModelState> = None;
        for (index, day, duration) in [
            (0, 0, 125),
            (1, 2, 250),
            (2, 8, 500),
            (3, 3, 700),
            (4, 30, 1000),
            (5, 31, 101),
        ] {
            let at = start + day * DAY_MS;
            let now = start + day.max(if index == 3 { 8 } else { day }) * DAY_MS;
            let mut measurements = Vec::new();
            for machine in 0..if with_environment { 2 } else { 1 } {
                let environment = if with_environment {
                    let profile = RunnerProfile {
                        version: 1,
                        os: "linux".into(),
                        arch: "x64".into(),
                        cpu_model: format!("CPU-{machine}"),
                        available_cpus: 4,
                        memory_mi_b: 8192,
                        cpu_quota_milli: None,
                        memory_limit_mi_b: None,
                        container_limits: "unknown".into(),
                        image: "".into(),
                        node_version: "24.0.0".into(),
                        package_manager: "yarn".into(),
                        package_manager_version: "4.11.0".into(),
                    };
                    Some(RunnerEnvironment {
                        fingerprint: digest_value(&profile).unwrap(),
                        profile,
                    })
                } else {
                    None
                };
                let mut row = observation(
                    &format!("task:{index}:{machine}"),
                    at,
                    duration * (machine + 1),
                );
                if index == 5 {
                    row.shard = Some(1);
                    row.total_shards = Some(2);
                }
                measurements.push(MeasurementArtifact {
                    version: VERSION,
                    scope: scope(),
                    run_id: (123 + index).to_string(),
                    run_attempt: 1,
                    observations: vec![row],
                    preparation_observations: vec![PreparationObservation {
                        execution_id: format!("prep:{index}:{machine}"),
                        observed_at_ms: at,
                        package_manager: "pnpm".into(),
                        package_manager_version: "10.0.0".into(),
                        install_mode: "focused".into(),
                        lockfile_digest: "a".repeat(64),
                        checkout_digest: "b".repeat(64),
                        workspace_set_digest: "c".repeat(64),
                        duration_ms: if index == 0 { 0 } else { duration + 200 },
                    }],
                    runner_environment: environment,
                });
            }
            // A delayed batch is validated against now; its observation time stays unchanged.
            let batch =
                compile_measurements(scope(), (123 + index).to_string(), 1, now, &measurements)
                    .unwrap();
            let (state, outcome) = apply_batch(prior.clone(), &batch, now).unwrap();
            let pruned = match outcome {
                ApplyOutcome::Applied { pruned_buckets } => pruned_buckets,
                _ => panic!("new batch was duplicate"),
            };
            let prediction = project_predictions(&state, now).unwrap();
            scenarios.push(json!({"now":now,"prior":prior,"measurements":measurements,"batch":batch,"batchDigest":batch.body_digest().unwrap(),"model":state,"prediction":prediction,"prunedBuckets":pruned}));
            prior = Some(state);
        }
    }
    let strings = json!({"unicode":"한글\u{2028}\u{2029}","html":"<>&/","control":"\u{0000}\u{0008}\u{000c}\n\r\t","integer":9007199254740991_u64});
    let pr = Scope {
        git_ref: ScopeRef::PullRequest {
            number: 42,
            head_repository_id: "12345".into(),
            head_ref: "refs/heads/기능".into(),
            base_ref: "refs/heads/main".into(),
        },
        ..scope()
    };
    println!("{}", serde_json::to_string_pretty(&json!({"scope":scope(),"scopeId":scope().id().unwrap(),"pullRequestScope":pr,"pullRequestScopeId":pr.id().unwrap(),"strings":strings,"stringsCanonical":String::from_utf8(canonical_bytes(&strings).unwrap()).unwrap(),"scenarios":scenarios})).unwrap());
}
