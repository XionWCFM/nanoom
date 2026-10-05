use nanoom::affected::WorkspaceEntry;
use nanoom::prediction::{
    apply_batch, compile_batch, digest_value, project_predictions, ModelState, PredictionContext,
    PredictionIndex, Scope, ScopeRef, SuccessfulObservation, DAY_MS,
};
use nanoom::scheduler::assign_with_prediction_index;

const START: u64 = 20_000 * DAY_MS;

fn scope(environment: &str) -> Scope {
    Scope {
        repository_key: "github-12345".into(),
        workflow_path: ".github/workflows/ci.yml".into(),
        git_ref: ScopeRef::Push {
            git_ref: "refs/heads/main".into(),
        },
        group: "ci".into(),
        task_runner: "turbo".into(),
        timing_environment: environment.into(),
    }
}

fn item(workspace: &str) -> WorkspaceEntry {
    WorkspaceEntry {
        group: "ci".into(),
        name: workspace.into(),
        path: format!("packages/{workspace}"),
        task: "build".into(),
        shard: None,
        total_shards: None,
        checkout_paths: vec![format!("packages/{workspace}")],
    }
}

fn learn(
    state: Option<ModelState>,
    scope: &Scope,
    run: usize,
    at: u64,
    samples: &[(&str, u64)],
) -> ModelState {
    let observations: Vec<_> = samples
        .iter()
        .enumerate()
        .map(|(index, (workspace, duration))| SuccessfulObservation {
            execution_id: format!("{run}-{index}"),
            observed_at_ms: at,
            group: scope.group.clone(),
            workspace: (*workspace).into(),
            task: "build".into(),
            shard: None,
            total_shards: None,
            task_runner: scope.task_runner.clone(),
            timing_environment: scope.timing_environment.clone(),
            duration_ms: *duration,
        })
        .collect();
    let batch = compile_batch(scope.clone(), (run + 1).to_string(), 1, at, &observations).unwrap();
    apply_batch(state, &batch, at).unwrap().0
}

fn schedule(
    states: &[ModelState],
    scope: &Scope,
    items: &[WorkspaceEntry],
    count: usize,
    at: u64,
) -> Vec<nanoom::scheduler::Assignment> {
    let index = PredictionIndex::from_tables(
        states
            .iter()
            .map(|state| project_predictions(state, at).unwrap()),
    )
    .unwrap();
    let context = PredictionContext {
        repository_key: scope.repository_key.clone(),
        workflow_path: scope.workflow_path.clone(),
        git_ref: scope.git_ref.clone(),
    };
    assign_with_prediction_index(
        "ci",
        items,
        count,
        &index,
        Some(&context),
        "turbo",
        &scope.timing_environment,
        (None, None),
        at,
    )
}

#[test]
fn same_task_with_fivefold_workspace_cost_is_balanced_by_exact_history() {
    let scope = scope("pool-fast");
    let state = learn(
        None,
        &scope,
        0,
        START,
        &[
            ("large-a", 5_000),
            ("large-b", 5_000),
            ("tiny-a", 1_000),
            ("tiny-b", 1_000),
        ],
    );
    let items: Vec<_> = ["large-a", "large-b", "tiny-a", "tiny-b"]
        .into_iter()
        .map(item)
        .collect();
    let assignments = schedule(&[state], &scope, &items, 2, START + 1);
    assert_eq!(assignments.len(), 2);
    for assignment in assignments {
        assert_eq!(assignment.predicted_duration_ms, 6_000);
        assert_eq!(assignment.prediction_sources.exact, 2);
        assert_eq!(assignment.prediction_sources.cold, 0);
    }
}

#[test]
fn hardware_fingerprints_separate_fast_slow_and_unknown_environments() {
    let environment = |cores, memory_gib| {
        digest_value(&serde_json::json!({
            "os":"linux", "arch":"x64", "cpu":"test-cpu", "cores":cores,
            "memoryGiB":memory_gib, "image":"ubuntu-24.04-20261001"
        }))
        .unwrap()
    };
    let fast = scope(&environment(16, 64));
    let slow = scope(&environment(2, 8));
    let unknown = scope(&environment(4, 16));
    let states = [
        learn(None, &fast, 0, START, &[("app", 1_000)]),
        learn(None, &slow, 1, START, &[("app", 12_000)]),
    ];
    for (scope, expected, cold) in [(&fast, 1_000, 0), (&slow, 12_000, 0), (&unknown, 1, 1)] {
        let assignments = schedule(&states, scope, &[item("app")], 1, START + 1);
        assert_eq!(assignments[0].predicted_duration_ms, expected);
        assert_eq!(assignments[0].prediction_sources.cold, cold);
    }
}

#[test]
fn missing_workspace_uses_explicit_group_fallback_and_expiry_returns_cold() {
    let scope = scope("pool");
    let state = learn(None, &scope, 0, START, &[("large", 10_000), ("tiny", 100)]);
    let assignment = schedule(
        std::slice::from_ref(&state),
        &scope,
        &[item("new")],
        1,
        START + 1,
    )
    .remove(0);
    assert_eq!(assignment.predicted_duration_ms, 5_050);
    assert_eq!(assignment.prediction_sources.group, 1);
    assert_eq!(assignment.prediction_sources.exact, 0);
    let assignment = schedule(&[state], &scope, &[item("large")], 1, START + 30 * DAY_MS).remove(0);
    assert_eq!(assignment.prediction_sources.cold, 1);
}

// These are prequential measurements: predict before ingesting the next sample.
// They report limitations, rather than asserting that a passing test means accuracy improved.
#[test]
fn next_run_error_is_measured_for_stable_outlier_shift_and_sparse_traces() {
    for (name, gap, values) in [
        ("stable", DAY_MS, vec![1000; 12]),
        (
            "jitter",
            DAY_MS,
            vec![900, 1100, 950, 1050, 1000, 900, 1100, 1000, 950, 1050],
        ),
        (
            "outlier",
            DAY_MS,
            vec![1000, 1000, 1000, 1000, 20000, 1000, 1000, 1000, 1000],
        ),
        (
            "step-up",
            DAY_MS,
            vec![1000, 1000, 1000, 1000, 8000, 8000, 8000, 8000, 8000],
        ),
        (
            "same-day-shift",
            1000,
            vec![1000, 1000, 1000, 1000, 8000, 8000, 8000, 8000, 8000],
        ),
        ("sparse", 12 * DAY_MS, vec![1000, 1000, 8000, 8000, 8000]),
    ] {
        let scope = scope("pool");
        let mut state: Option<ModelState> = None;
        let mut absolute_error = 0;
        let mut baseline_error = 0;
        let mut median_error = 0;
        let mut latest_error = 0;
        let mut ewma_error = 0;
        let mut recent = Vec::<u64>::new();
        let mut ewma = 0_u64;
        let mut actual_total = 0;
        let mut measured = 0;
        for (run, actual) in values.into_iter().enumerate() {
            let at = START + run as u64 * gap;
            if let Some(previous) = &state {
                // Replay the released daily-only estimator on identical retained buckets.
                let mut legacy: ModelState = previous.clone();
                for entry in &mut legacy.entries {
                    entry.recent_batches.clear();
                    let latest_day = entry.buckets.last().unwrap().0;
                    let numerator: f64 = entry
                        .buckets
                        .iter()
                        .map(|bucket| {
                            bucket.2 as f64 * 2_f64.powf(-((latest_day - bucket.0) as f64) / 7.0)
                        })
                        .sum();
                    let denominator: f64 = entry
                        .buckets
                        .iter()
                        .map(|bucket| {
                            bucket.1 as f64 * 2_f64.powf(-((latest_day - bucket.0) as f64) / 7.0)
                        })
                        .sum();
                    entry.prediction.0 = (numerator / denominator + 0.5).floor() as u64;
                }
                let baseline = schedule(&[legacy], &scope, &[item("app")], 1, at).remove(0);
                baseline_error += baseline.predicted_duration_ms.abs_diff(actual);
                let assignment = schedule(
                    std::slice::from_ref(previous),
                    &scope,
                    &[item("app")],
                    1,
                    at,
                )
                .remove(0);
                absolute_error += assignment.predicted_duration_ms.abs_diff(actual);
                let mut window = recent.iter().rev().take(3).copied().collect::<Vec<_>>();
                window.sort_unstable();
                let middle = window.len() / 2;
                let median = if window.len() % 2 == 0 {
                    (window[middle - 1] + window[middle]) / 2
                } else {
                    window[middle]
                };
                median_error += median.abs_diff(actual);
                latest_error += recent.last().unwrap().abs_diff(actual);
                ewma_error += ewma.abs_diff(actual);
                actual_total += actual;
                measured += 1;
            }
            state = Some(learn(state, &scope, run, at, &[("app", actual)]));
            ewma = if recent.is_empty() {
                actual
            } else {
                (ewma + actual) / 2
            };
            recent.push(actual);
        }
        assert!(measured >= 4);
        if name == "stable" {
            assert_eq!(absolute_error, 0);
        }
        // Allow at most 0.5 percentage points of jitter tradeoff; the other
        // adversarial traces must improve, including same-day and sparse runs.
        if name == "jitter" {
            assert!(absolute_error * 200 <= baseline_error * 200 + actual_total);
        } else if name != "stable" {
            assert!(absolute_error < baseline_error, "{name} did not improve");
        }
        println!(
            "{name}: next-run WAPE baseline={:.2}%, candidate={:.2}%, median3={:.2}%, latest={:.2}%, EWMA0.5={:.2}% ({measured} predictions)",
            100.0 * baseline_error as f64 / actual_total as f64,
            100.0 * absolute_error as f64 / actual_total as f64,
            100.0 * median_error as f64 / actual_total as f64,
            100.0 * latest_error as f64 / actual_total as f64,
            100.0 * ewma_error as f64 / actual_total as f64,
        );
    }
}
