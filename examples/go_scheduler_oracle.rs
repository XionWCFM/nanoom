//! Frozen migration oracle; regenerate with cargo run --example go_scheduler_oracle.
use nanoom::affected::{AffectedOutput, GroupOutput, WorkspaceEntry};
use nanoom::config::{DistributionConfig, DistributionTier};
use nanoom::prediction::*;
use serde_json::json;
use std::collections::HashMap;
fn main() {
    let context = PredictionContext {
        repository_key: "github-12345".into(),
        workflow_path: ".github/workflows/ci.yml".into(),
        git_ref: ScopeRef::Push {
            git_ref: "refs/heads/main".into(),
        },
    };
    let mut scenarios = Vec::new();
    for mode in [
        "cold",
        "warm",
        "preparation",
        "fallback",
        "shards",
        "labels",
        "expired",
    ] {
        let environment = if mode == "labels" {
            "runner-labels:[\"ubuntu-latest\"]"
        } else {
            "env"
        };
        let scope = context.scope(context.git_ref.clone(), "ci", "npm", environment);
        let items: Vec<_> = (0..4)
            .map(|i| WorkspaceEntry {
                group: "ci".into(),
                name: format!("pkg-{i}"),
                path: format!("packages/pkg-{i}"),
                task: "test".into(),
                shard: (mode == "shards").then_some(1),
                total_shards: (mode == "shards").then_some(2),
                checkout_paths: vec![format!("packages/pkg-{i}"), "packages/shared".into()],
            })
            .collect();
        let mut rows = Vec::new();
        if mode != "cold" {
            for (i, item) in items.iter().enumerate() {
                if mode == "fallback" {
                    break;
                }
                let key = PredictionKey::TaskExact {
                    group: "ci".into(),
                    workspace: item.name.clone(),
                    task: "test".into(),
                    shard: item.shard.map(|n| n as u64),
                    total_shards: item.total_shards.map(|n| n as u64),
                    task_runner: "npm".into(),
                    timing_environment: environment.into(),
                }
                .id()
                .unwrap();
                rows.push(PredictionRow(
                    key,
                    if i == 0 { 1000 } else { 10 },
                    2,
                    100_000,
                    if mode == "expired" { 100_001 } else { 200_000 },
                ));
            }
            let key = PredictionKey::TaskFallback {
                group: "ci".into(),
                task: "test".into(),
                shard: None,
                total_shards: None,
                task_runner: "npm".into(),
                timing_environment: environment.into(),
            }
            .id()
            .unwrap();
            rows.push(PredictionRow(key, 100, 3, 100_000, 200_000));
            if mode == "preparation" {
                let key = PredictionKey::PreparationFallback {
                    group: "ci".into(),
                    task_runner: "npm".into(),
                    timing_environment: environment.into(),
                    package_manager: "npm".into(),
                    package_manager_version: "11.16.0".into(),
                    install_mode: "focused".into(),
                    lockfile_digest: "a".repeat(64),
                }
                .id()
                .unwrap();
                rows.push(PredictionRow(key, 100, 4, 100_000, 200_000));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let table = PredictionTable {
            version: 3,
            scope,
            model_updated_at_ms: 100_000,
            rows,
            environment_predictions: vec![],
        };
        let index = PredictionIndex::from_tables([table.clone()]).unwrap();
        let labels = (mode == "labels").then(|| vec!["ubuntu-latest".into()]);
        let tier = |percent, concurrency| DistributionTier {
            max_affected_percent: percent,
            concurrency,
            runner_labels: labels.clone(),
            timing_environment: None,
        };
        let distribution = nanoom::scheduler::select_tier(
            &DistributionConfig {
                small: tier(20.0, 1),
                medium: tier(60.0, 2),
                full: tier(100.0, 4),
            },
            100.0,
        );
        let group = GroupOutput {
            label: "ci".into(),
            workspaces: items.clone(),
            total_workspaces: 4,
            affected_workspaces: 4,
            affected_percent: 100.0,
            distribution: Some(distribution),
            runner_labels: None,
            timing_environment: None,
        };
        let output = AffectedOutput {
            group: HashMap::from([("ci".into(), group)]),
            has_change: true,
            diagnostics: None,
        };
        let prep = PreparationContext {
            package_manager: "npm".into(),
            package_manager_version: "11.16.0".into(),
            install_mode: "focused".into(),
            lockfile_digest: "a".repeat(64),
        };
        let now = if mode == "expired" { 200_000 } else { 100_001 };
        let matrix = nanoom::affected::generate_matrix_with_prediction_index_and_preparation(
            &output,
            &index,
            Some(&context),
            "npm",
            "env",
            Some(&prep),
            now,
        );
        let item_values: Vec<_> = items
            .iter()
            .map(|item| {
                let mut value = serde_json::to_value(item).unwrap();
                value["checkoutPaths"] = json!(item.checkout_paths);
                value
            })
            .collect();
        scenarios.push(json!({"mode":mode,"context":context,"table":table,"preparation":prep,"now":now,"items":item_values,"matrix":matrix,"labels":labels}));
    }
    println!("{}", serde_json::to_string_pretty(&scenarios).unwrap());
}
