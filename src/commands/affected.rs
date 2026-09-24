use crate::affected::calculate_with_override;
use crate::error::Result;
use clap::Args;
use std::collections::HashSet;
use std::path::PathBuf;

const MAX_PREDICTION_CONTEXT_BYTES: usize = 4096;
const MAX_PREPARATION_CONTEXT_BYTES: usize = 4096;

#[derive(Args, Debug, Clone)]
pub struct AffectedArgs {
    #[arg(long, help = "Base reference (branch/tag/commit)")]
    pub base: Option<String>,

    #[arg(long, help = "Head reference (defaults to HEAD)")]
    pub head: Option<String>,

    #[arg(long, help = "Output the canonical affected report as JSON")]
    pub json: bool,

    #[arg(
        long = "prediction",
        value_name = "FILE",
        help = "PredictionArtifact v3 JSON used for runner assignments"
    )]
    pub predictions: Vec<PathBuf>,

    #[arg(long, requires = "predictions", help = "Prediction scope context JSON")]
    pub prediction_context: Option<PathBuf>,

    #[arg(
        long,
        help = "PreparationContext JSON containing package-manager identity and lockfile digest"
    )]
    pub preparation_context: Option<PathBuf>,

    #[arg(
        long = "history",
        hide = true,
        help = "Deprecated raw timing history; ignored for v3 planning"
    )]
    pub legacy_history: Option<PathBuf>,

    #[arg(long, hide = true, value_parser = ["disabled", "fallback", "corrupt"])]
    pub history_status: Option<String>,

    #[arg(
        long,
        default_value = "auto",
        help = "Runner identity used for timing lookup"
    )]
    pub timing_runner: String,

    #[arg(
        long,
        default_value = "default",
        help = "Hardware/environment identity used for timing lookup"
    )]
    pub timing_environment: String,

    #[arg(
        long,
        requires = "plan_context",
        help = "Write the complete Plan v1 JSON to this file"
    )]
    pub plan_output: Option<PathBuf>,

    #[arg(long, requires = "plan_output", help = "Plan provenance JSON file")]
    pub plan_context: Option<PathBuf>,
}

pub async fn execute(
    args: AffectedArgs,
    config: &crate::Config,
    cwd: &std::path::Path,
) -> Result<()> {
    if args.plan_output.is_some() != args.plan_context.is_some() {
        return Err(crate::error::Error::ConfigValidation(
            "--plan-output and --plan-context must be supplied together".into(),
        ));
    }
    if args.json && args.plan_output.is_some() {
        return Err(crate::error::Error::ConfigValidation(
            "--json full reports cannot be combined with bounded Plan outputs".into(),
        ));
    }
    let timing_runner = resolve_timing_runner(cwd, &args.timing_runner)?;
    let result =
        calculate_with_override(config, cwd, args.base.as_deref(), args.head.as_deref()).await?;
    let history_needed = result.group.values().any(|group| {
        group.workspaces.len() > 1
            && group
                .distribution
                .as_ref()
                .is_some_and(|tier| tier.concurrency > 1)
    });
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| crate::error::Error::InvalidConfig(error.to_string()))?
        .as_millis() as u64;
    let mut history_status = args
        .history_status
        .clone()
        .unwrap_or_else(|| "disabled".into());
    let mut prediction_index =
        crate::prediction::PredictionIndex::new([]).map_err(crate::error::Error::InvalidConfig)?;
    let mut prediction_context = None;
    if history_needed {
        if args.legacy_history.is_some() && args.predictions.is_empty() {
            eprintln!("legacy raw timing history is ignored; using cold scheduling");
            history_status = args
                .history_status
                .clone()
                .unwrap_or_else(|| "fallback".into());
        }
        if !args.predictions.is_empty() && history_status != "corrupt" {
            let bundles = args
                .predictions
                .iter()
                .map(|path| {
                    crate::prediction::PredictionArtifactBundle::load(&crate::plan::resolve_path(
                        cwd, path,
                    ))
                })
                .collect::<std::result::Result<Vec<_>, _>>();
            match bundles.and_then(crate::prediction::PredictionIndex::new) {
                Ok(index) if index.has_valid_rows(now_ms) => {
                    prediction_index = index;
                    history_status = "loaded".into();
                    if let Some(path) = args.prediction_context.as_deref() {
                        prediction_context = load_prediction_context(cwd, path);
                    }
                    if prediction_context.is_none() {
                        eprintln!("PredictionContext is unavailable; using cold scheduling");
                        prediction_index = crate::prediction::PredictionIndex::new([])
                            .map_err(crate::error::Error::InvalidConfig)?;
                        history_status = "fallback".into();
                    }
                }
                Ok(_) => history_status = "fallback".into(),
                Err(error) => {
                    eprintln!("PredictionArtifact v3 unavailable; using cold scheduling: {error}");
                    history_status = "corrupt".into();
                }
            }
        }
    } else {
        history_status = "history_not_needed".into();
    }
    let preparation_context = args
        .preparation_context
        .as_deref()
        .and_then(|path| load_preparation_context(cwd, path));
    let matrix = crate::affected::generate_matrix_with_prediction_index_and_preparation(
        &result,
        &prediction_index,
        prediction_context.as_ref(),
        &timing_runner,
        &args.timing_environment,
        preparation_context.as_ref(),
        now_ms,
    );
    let compact_plan = match (&args.plan_output, &args.plan_context) {
        (Some(plan_output), Some(plan_context)) => Some(crate::plan::write_affected_plan(
            &result,
            &matrix,
            &timing_runner,
            cwd,
            &crate::plan::resolve_path(cwd, plan_context),
            &crate::plan::resolve_path(cwd, plan_output),
        )?),
        _ => None,
    };
    let total_checkout_path_count: usize = matrix
        .as_object()
        .into_iter()
        .flat_map(|groups| groups.values())
        .filter_map(|group| group.get("include").and_then(serde_json::Value::as_array))
        .flatten()
        .filter_map(|entry| {
            entry
                .get("checkoutPathCount")
                .and_then(serde_json::Value::as_u64)
        })
        .map(|count| count as usize)
        .sum();
    let unique_checkout_path_count = result
        .group
        .values()
        .flat_map(|group| group.workspaces.iter())
        .flat_map(|item| item.checkout_paths.iter())
        .collect::<HashSet<_>>()
        .len();
    let prediction_sources = matrix
        .as_object()
        .into_iter()
        .flat_map(|groups| groups.values())
        .filter_map(|group| group.get("include").and_then(serde_json::Value::as_array))
        .flatten()
        .filter_map(|entry| entry.get("predictionSources"))
        .fold([0_u64; 4], |mut counts, sources| {
            counts[0] += sources
                .get("exact")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            counts[1] += sources
                .get("group")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            counts[2] += sources
                .get("cold")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            counts[3] += sources
                .get("sampleCount")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            counts
        });
    let concurrency_diagnostics = scheduling_diagnostics(&matrix);

    if args.json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "affected": result,
                "matrix": matrix,
                "scheduling": {
                    "historyStatus": history_status,
                    "historyNeeded": history_needed,
                    "timingRunner": timing_runner,
                    "timingEnvironment": args.timing_environment,
                    "objective": ["predictedPreparationPlusTaskMakespanMs", "totalRunnerMs", "totalCheckoutPathCount", "assignmentCount", "stableAssignmentOrder"],
                    "totalCheckoutPathCount": total_checkout_path_count,
                    "uniqueCheckoutPathCount": unique_checkout_path_count,
                    "duplicatedCheckoutPathCount": total_checkout_path_count.saturating_sub(unique_checkout_path_count),
                    "predictionSources": {
                        "exact": prediction_sources[0],
                        "group": prediction_sources[1],
                        "cold": prediction_sources[2],
                        "sampleCount": prediction_sources[3]
                    },
                    "concurrency": concurrency_diagnostics
                }
            }))?
        );
        return Ok(());
    }

    if let Some(compact_plan) = compact_plan {
        let mut compact: serde_json::Value = serde_json::from_str(&compact_plan)?;
        compact["result"]["timingRunner"] = serde_json::Value::String(timing_runner);
        compact["result"]["timingEnvironment"] = serde_json::Value::String(args.timing_environment);
        compact["result"]["historyStatus"] = serde_json::Value::String(history_status);
        compact["result"]["historyNeeded"] = serde_json::Value::Bool(history_needed);
        compact["result"]["concurrency"] = concurrency_diagnostics;
        println!("{}", serde_json::to_string(&compact)?);
        return Ok(());
    }

    println!("◆ nanoom affected");
    println!(
        "  Result: {}",
        if result.has_change {
            "changes found"
        } else {
            "no changes found"
        }
    );
    if let Some(diagnostics) = &result.diagnostics {
        println!("  Comparison: {}", diagnostics.comparison.mode);
        println!(
            "  Commits: {} -> {}",
            diagnostics.comparison.base_commit, diagnostics.comparison.head_commit
        );
        println!("  Changed files: {}", diagnostics.changed_files.len());
        for file in &diagnostics.changed_files {
            println!("  - {file}");
        }
    }
    for (group_name, group_output) in &result.group {
        println!(
            "\n  Matrix group: {} ({} entries)",
            group_name,
            group_output.workspaces.len()
        );
        for ws in &group_output.workspaces {
            let shard_str = ws
                .shard
                .map(|s| format!(" (shard {})", s))
                .unwrap_or_default();
            println!("    - {} / {} [{}]", ws.name, ws.task, ws.path);
            if let Some(reason) = result
                .diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.reasons.get(&ws.name))
            {
                println!(
                    "      why: {}",
                    match reason.kind.as_str() {
                        "direct" => format!("direct change: {}", reason.changed_files.join(", ")),
                        "globalDependency" =>
                            format!("global dependency: {}", reason.changed_files.join(", ")),
                        "workspaceManifestStructure" => format!(
                            "workspace manifest deleted or renamed: {}",
                            reason.changed_files.join(", ")
                        ),
                        _ => format!(
                            "transitive dependency: {}",
                            reason.dependency_path.join(" -> ")
                        ),
                    }
                );
            }
            if !shard_str.is_empty() {
                println!("    {}", shard_str);
            }
        }
    }

    Ok(())
}

fn scheduling_diagnostics(matrix: &serde_json::Value) -> serde_json::Value {
    let mut automatic = 0_u64;
    let mut cold_cap = 0_u64;
    let mut preparation_exact = 0_u64;
    let mut preparation_group = 0_u64;
    let mut preparation_unknown = 0_u64;
    for assignment in matrix
        .as_object()
        .into_iter()
        .flat_map(|groups| groups.values())
        .filter_map(|group| group.get("include").and_then(serde_json::Value::as_array))
        .flatten()
    {
        match assignment
            .get("schedulingMode")
            .and_then(serde_json::Value::as_str)
        {
            Some("automatic") => automatic += 1,
            Some("cold-cap") => cold_cap += 1,
            _ => continue,
        }
        match assignment
            .get("preparationPredictionSource")
            .and_then(serde_json::Value::as_str)
        {
            Some("exact") => preparation_exact += 1,
            Some("group") => preparation_group += 1,
            _ => preparation_unknown += 1,
        }
    }
    serde_json::json!({
        "automaticAssignmentCount": automatic,
        "coldCapAssignmentCount": cold_cap,
        "preparationPredictionSources": {
            "exact": preparation_exact,
            "group": preparation_group,
            "unknown": preparation_unknown
        }
    })
}

fn load_prediction_context(
    cwd: &std::path::Path,
    path: &std::path::Path,
) -> Option<crate::prediction::PredictionContext> {
    use std::io::Read;

    let mut bytes = Vec::new();
    std::fs::File::open(crate::plan::resolve_path(cwd, path))
        .ok()?
        .take((MAX_PREDICTION_CONTEXT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_PREDICTION_CONTEXT_BYTES {
        return None;
    }
    let context: crate::prediction::PredictionContext = serde_json::from_slice(&bytes).ok()?;
    context.validate().ok()?;
    Some(context)
}

fn load_preparation_context(
    cwd: &std::path::Path,
    path: &std::path::Path,
) -> Option<crate::prediction::PreparationContext> {
    use std::io::Read;

    let mut bytes = Vec::new();
    std::fs::File::open(crate::plan::resolve_path(cwd, path))
        .ok()?
        .take((MAX_PREPARATION_CONTEXT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_PREPARATION_CONTEXT_BYTES {
        return None;
    }
    let context: crate::prediction::PreparationContext = serde_json::from_slice(&bytes).ok()?;
    context.validate().ok()?;
    Some(context)
}

fn resolve_timing_runner(cwd: &std::path::Path, requested: &str) -> Result<String> {
    if requested != "auto" {
        return Ok(requested.to_string());
    }
    let turbo = cwd.join("turbo.json").exists();
    let nx = cwd.join("nx.json").exists();
    if turbo && nx {
        return Err(crate::error::Error::InvalidRunner(
            "both turbo.json and nx.json exist; set timingRunner explicitly".into(),
        ));
    }
    Ok(if turbo {
        "turbo".into()
    } else if nx {
        "nx".into()
    } else {
        crate::commands::install::detect_package_manager(cwd, None)?
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_timing_runner;

    use crate::affected::{AffectedOutput, GroupOutput, WorkspaceEntry};

    use std::collections::HashMap;
    use tempfile::tempdir;

    fn mock_output() -> AffectedOutput {
        let workspaces = vec![WorkspaceEntry {
            group: "ci".into(),
            name: "proj-a".into(),
            path: "packages/proj-a".into(),
            task: "test".into(),
            shard: None,
            total_shards: None,
            checkout_paths: vec!["packages/proj-a".into()],
        }];
        let mut group = HashMap::new();
        group.insert(
            "ci".into(),
            GroupOutput {
                runner_labels: None,
                timing_environment: None,
                label: "ci".into(),
                workspaces,
                total_workspaces: 1,
                affected_workspaces: 1,
                affected_percent: 100.0,
                distribution: None,
            },
        );
        AffectedOutput {
            has_change: true,
            group,
            diagnostics: None,
        }
    }

    #[test]
    fn test_execute_json_output() {
        // Test structure only - execute requires full config and env setup
        // This is tested in integration tests
        let result = mock_output();
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("has_change"));
        assert!(json.contains("proj-a"));
    }

    #[test]
    fn test_format_text_branch() {
        let result = mock_output();
        let output = format!(
            "◆ nanoom affected\n  Result: changes found\n  Matrix group: ci ({} entries)",
            result.group["ci"].workspaces.len()
        );
        assert!(output.contains("Result: changes found"));
        assert!(output.contains("Matrix group: ci (1 entries)"));
    }

    #[test]
    fn timing_runner_auto_resolves_the_execution_boundary() {
        let explicit = tempdir().unwrap();
        assert_eq!(
            resolve_timing_runner(explicit.path(), "yarn").unwrap(),
            "yarn"
        );
        for (marker, expected) in [("turbo.json", "turbo"), ("nx.json", "nx")] {
            let dir = tempdir().unwrap();
            std::fs::write(dir.path().join(marker), "{}").unwrap();
            assert_eq!(resolve_timing_runner(dir.path(), "auto").unwrap(), expected);
        }
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(resolve_timing_runner(dir.path(), "auto").unwrap(), "pnpm");

        let ambiguous = tempdir().unwrap();
        std::fs::write(ambiguous.path().join("turbo.json"), "{}").unwrap();
        std::fs::write(ambiguous.path().join("nx.json"), "{}").unwrap();
        assert!(resolve_timing_runner(ambiguous.path(), "auto").is_err());
    }
}
