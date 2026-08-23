use crate::config::Config;
use crate::error::Result;
use crate::git::{resolve_base_commit, ComparisonMode, GitEvent, GitRepo};
use crate::workspace::{apply_rules, calculate_affected, Workspace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedOutput {
    pub group: HashMap<String, GroupOutput>,
    pub has_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupOutput {
    pub label: String,
    pub workspaces: Vec<WorkspaceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub group: String,
    pub name: String,
    pub path: String,
    pub task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard: Option<usize>,
    #[serde(rename = "totalShards", skip_serializing_if = "Option::is_none")]
    pub total_shards: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolate: Option<bool>,
}

pub async fn calculate(config: &Config, cwd: &Path) -> Result<AffectedOutput> {
    calculate_with_override(config, cwd, None, None).await
}

pub async fn calculate_with_override(
    config: &Config,
    cwd: &Path,
    base: Option<&str>,
    head: Option<&str>,
) -> Result<AffectedOutput> {
    let git_root = crate::git::detect_git_root(cwd)?;
    let git = GitRepo::open(&git_root)?;

    let event = match (base, head) {
        (Some(base), Some(head)) => GitEvent::PullRequest {
            base_ref: base.to_string(),
            head_ref: head.to_string(),
        },
        (Some(base), None) => GitEvent::Push {
            ref_name: base.to_string(),
        },
        (None, _) => GitEvent::from_env()?,
    };
    let mode = ComparisonMode::from_env();
    let base_commit = resolve_base_commit(&git, &event, mode)?;

    let head_ref = event.head_ref();
    let changed_files = git.get_changed_files(&base_commit, Some(head_ref))?;

    let workspace = Workspace::discover(config, cwd)?;

    let mut group_outputs = HashMap::new();
    let mut has_any_change = false;

    for (group_name, group_config) in &config.group {
        let affected_projects = calculate_affected(
            &workspace,
            &changed_files,
            &config.global_dependencies,
            cwd,
            true,
        );

        let filtered_projects =
            apply_rules(affected_projects, &group_config.rules, &group_config.tasks);

        if filtered_projects.is_empty() {
            group_outputs.insert(
                group_name.clone(),
                GroupOutput {
                    label: group_name.clone(),
                    workspaces: vec![],
                },
            );
            continue;
        }

        has_any_change = true;

        let mut workspaces = Vec::new();

        for task in &group_config.tasks {
            for project in &filtered_projects {
                let rule = group_config.rules.iter().find(|r| r.name == project.name);

                let is_isolated = rule.map(|r| r.isolate.contains(task)).unwrap_or(false);
                let shard_rule = rule.and_then(|r| r.shard.iter().find(|s| s.task == *task));

                if let Some(shard_rule) = shard_rule {
                    for shard_idx in 1..=shard_rule.shard {
                        workspaces.push(WorkspaceEntry {
                            group: group_name.clone(),
                            name: project.name.clone(),
                            path: project.path.to_string_lossy().to_string(),
                            task: task.clone(),
                            shard: Some(shard_idx),
                            total_shards: Some(shard_rule.shard),
                            isolate: Some(false),
                        });
                    }
                } else if is_isolated {
                    workspaces.push(WorkspaceEntry {
                        group: group_name.clone(),
                        name: project.name.clone(),
                        path: project.path.to_string_lossy().to_string(),
                        task: task.clone(),
                        shard: None,
                        total_shards: None,
                        isolate: Some(true),
                    });
                } else {
                    workspaces.push(WorkspaceEntry {
                        group: group_name.clone(),
                        name: project.name.clone(),
                        path: project.path.to_string_lossy().to_string(),
                        task: task.clone(),
                        shard: None,
                        total_shards: None,
                        isolate: Some(false),
                    });
                }
            }
        }

        group_outputs.insert(
            group_name.clone(),
            GroupOutput {
                label: group_name.clone(),
                workspaces,
            },
        );
    }

    Ok(AffectedOutput {
        group: group_outputs,
        has_change: has_any_change,
    })
}

pub fn generate_matrix(output: &AffectedOutput) -> serde_json::Value {
    let mut matrix = serde_json::Map::new();

    for (group_name, group_output) in &output.group {
        let include: Vec<serde_json::Value> = group_output
            .workspaces
            .iter()
            .map(|w| {
                let mut entry = serde_json::json!({
                    "group": group_name,
                    "label": group_output.label,
                    "name": w.name,
                    "path": w.path,
                    "task": w.task,
                });
                if let Some(shard) = w.shard {
                    entry["shard"] = serde_json::Value::Number(shard.into());
                }
                if let Some(total) = w.total_shards {
                    entry["totalShards"] = serde_json::Value::Number(total.into());
                }
                if let Some(isolate) = w.isolate {
                    entry["isolate"] = serde_json::Value::Bool(isolate);
                }
                entry
            })
            .collect();

        matrix.insert(
            group_name.clone(),
            serde_json::json!({ "include": include }),
        );
    }

    serde_json::Value::Object(matrix)
}

pub fn generate_matrix_for_group(output: &AffectedOutput, group_name: &str) -> serde_json::Value {
    if let Some(group_output) = output.group.get(group_name) {
        let include: Vec<serde_json::Value> = group_output
            .workspaces
            .iter()
            .map(|w| {
                let mut entry = serde_json::json!({
                    "group": group_name,
                    "label": group_output.label,
                    "name": w.name,
                    "path": w.path,
                    "task": w.task,
                });
                if let Some(shard) = w.shard {
                    entry["shard"] = serde_json::Value::Number(shard.into());
                }
                if let Some(total) = w.total_shards {
                    entry["totalShards"] = serde_json::Value::Number(total.into());
                }
                if let Some(isolate) = w.isolate {
                    entry["isolate"] = serde_json::Value::Bool(isolate);
                }
                entry
            })
            .collect();

        serde_json::json!({ "include": include })
    } else {
        serde_json::json!({ "include": [] })
    }
}
