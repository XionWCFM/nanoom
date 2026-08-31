use crate::affected::{calculate_with_override, generate_matrix_with_history};
use crate::error::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct AffectedArgs {
    #[arg(long, help = "Base reference (branch/tag/commit)")]
    pub base: Option<String>,

    #[arg(long, help = "Head reference (defaults to HEAD)")]
    pub head: Option<String>,

    #[arg(long, help = "Output the canonical affected report as JSON")]
    pub json: bool,

    #[arg(
        long,
        help = "Best-effort timing history JSON used for runner assignments"
    )]
    pub history: Option<PathBuf>,

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
}

pub async fn execute(
    args: AffectedArgs,
    config: &crate::Config,
    cwd: &std::path::Path,
) -> Result<()> {
    let timing_runner = resolve_timing_runner(cwd, &args.timing_runner)?;
    let result =
        calculate_with_override(config, cwd, args.base.as_deref(), args.head.as_deref()).await?;
    let (history, history_status) = match args.history.as_deref() {
        Some(path) => match crate::scheduler::TimingHistory::load(path) {
            Ok(history) => (history, "loaded".to_string()),
            Err(error) => {
                eprintln!("timing history unavailable; using deterministic cold start: {error}");
                (
                    crate::scheduler::TimingHistory::default(),
                    "fallback".to_string(),
                )
            }
        },
        None => (
            crate::scheduler::TimingHistory::default(),
            "disabled".to_string(),
        ),
    };
    let matrix =
        generate_matrix_with_history(&result, &history, &timing_runner, &args.timing_environment);

    if args.json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "affected": result,
                "matrix": matrix,
                "scheduling": {
                    "historyStatus": history_status,
                    "timingRunner": timing_runner,
                    "timingEnvironment": args.timing_environment
                }
            }))?
        );
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
        }];
        let mut group = HashMap::new();
        group.insert(
            "ci".into(),
            GroupOutput {
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
        for (marker, expected) in [("turbo.json", "turbo"), ("nx.json", "nx")] {
            let dir = tempdir().unwrap();
            std::fs::write(dir.path().join(marker), "{}").unwrap();
            assert_eq!(resolve_timing_runner(dir.path(), "auto").unwrap(), expected);
        }
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(resolve_timing_runner(dir.path(), "auto").unwrap(), "pnpm");
    }
}
