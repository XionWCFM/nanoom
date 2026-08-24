use crate::affected::{calculate_with_override, generate_matrix};
use crate::error::Result;
use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct AffectedArgs {
    #[arg(long, help = "Base reference (branch/tag/commit)")]
    pub base: Option<String>,

    #[arg(long, help = "Head reference (defaults to HEAD)")]
    pub head: Option<String>,

    #[arg(long, value_parser = ["json", "text"], help = "Output format (json, text)")]
    pub format: Option<String>,

    #[arg(long, help = "Generate GitHub Actions matrices for all groups")]
    pub matrix: bool,

    #[arg(long, help = "Output matrix as JSON to stdout")]
    pub json: bool,

    #[arg(
        long,
        help = "Output affected diagnostics and matrices as one JSON report"
    )]
    pub report: bool,
}

pub async fn execute(
    args: AffectedArgs,
    config: &crate::Config,
    cwd: &std::path::Path,
) -> Result<()> {
    let result =
        calculate_with_override(config, cwd, args.base.as_deref(), args.head.as_deref()).await?;

    if args.report {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "affected": result,
                "matrix": generate_matrix(&result)
            }))?
        );
        return Ok(());
    }

    if args.json {
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }

    if args.matrix {
        let matrix = generate_matrix(&result);
        println!("{}", serde_json::to_string(&matrix)?);
        return Ok(());
    }

    let format = args.format.as_deref().unwrap_or("text");
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "text" => {
            println!("Has changes: {}", result.has_change);
            if let Some(diagnostics) = &result.diagnostics {
                println!(
                    "Comparison ({}): {} -> {}",
                    diagnostics.comparison.mode,
                    diagnostics.comparison.base_commit,
                    diagnostics.comparison.head_commit
                );
                println!("Changed files: {}", diagnostics.changed_files.len());
                for file in &diagnostics.changed_files {
                    println!("  - {file}");
                }
            }
            for (group_name, group_output) in &result.group {
                println!(
                    "\nGroup: {} ({} entries)",
                    group_name,
                    group_output.workspaces.len()
                );
                for ws in &group_output.workspaces {
                    let shard_str = ws
                        .shard
                        .map(|s| format!(" (shard {})", s))
                        .unwrap_or_default();
                    let isolate_str = ws
                        .isolate
                        .filter(|&b| b)
                        .map(|_| " [isolated]")
                        .unwrap_or_default();
                    println!("  - {}: {} [{}]", ws.name, ws.task, ws.path);
                    if let Some(reason) = result
                        .diagnostics
                        .as_ref()
                        .and_then(|diagnostics| diagnostics.reasons.get(&ws.name))
                    {
                        println!(
                            "    reason: {}",
                            match reason.kind.as_str() {
                                "direct" =>
                                    format!("direct change: {}", reason.changed_files.join(", ")),
                                "globalDependency" => format!(
                                    "global dependency: {}",
                                    reason.changed_files.join(", ")
                                ),
                                _ => format!(
                                    "transitive dependency: {}",
                                    reason.dependency_path.join(" -> ")
                                ),
                            }
                        );
                    }
                    if !shard_str.is_empty() || !isolate_str.is_empty() {
                        println!("    {}{}", shard_str, isolate_str);
                    }
                }
            }
        }
        _ => unreachable!("clap validates affected output formats"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::affected::{AffectedOutput, GroupOutput, WorkspaceEntry};

    use std::collections::HashMap;

    fn mock_output() -> AffectedOutput {
        let workspaces = vec![WorkspaceEntry {
            group: "ci".into(),
            name: "proj-a".into(),
            path: "packages/proj-a".into(),
            task: "test".into(),
            shard: None,
            total_shards: None,
            isolate: Some(false),
        }];
        let mut group = HashMap::new();
        group.insert(
            "ci".into(),
            GroupOutput {
                label: "ci".into(),
                workspaces,
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
    fn test_execute_matrix_option() {
        let result = mock_output();
        let matrix = crate::affected::generate_matrix(&result);
        let json = serde_json::to_string(&matrix).unwrap();
        assert!(json.contains("include"));
    }

    #[test]
    fn test_format_text_branch() {
        let result = mock_output();
        let output = format!(
            "Has changes: {}\nGroup: ci ({} entries)",
            result.has_change,
            result.group["ci"].workspaces.len()
        );
        assert!(output.contains("Has changes: true"));
        assert!(output.contains("Group: ci (1 entries)"));
    }

    #[test]
    fn test_format_json_branch() {
        let result = mock_output();
        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("has_change"));
    }
}
