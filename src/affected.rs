use crate::config::Config;
use crate::error::Result;
use crate::git::{resolve_base_commit, ComparisonMode, GitEvent, GitRepo};
use crate::workspace::{apply_rules, calculate_affected, Workspace};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedOutput {
    pub group: HashMap<String, GroupOutput>,
    pub has_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<AffectedDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedDiagnostics {
    pub comparison: ComparisonDetails,
    #[serde(rename = "changedFiles")]
    pub changed_files: Vec<String>,
    pub reasons: BTreeMap<String, AffectedReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonDetails {
    pub mode: String,
    #[serde(rename = "requestedBase")]
    pub requested_base: String,
    #[serde(rename = "requestedHead")]
    pub requested_head: String,
    #[serde(rename = "baseCommit")]
    pub base_commit: String,
    #[serde(rename = "headCommit")]
    pub head_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedReason {
    pub kind: String,
    #[serde(rename = "changedFiles", skip_serializing_if = "Vec::is_empty")]
    pub changed_files: Vec<String>,
    #[serde(rename = "dependencyPath", skip_serializing_if = "Vec::is_empty")]
    pub dependency_path: Vec<String>,
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

    let event = match base {
        Some(base) => match head {
            Some(head) => GitEvent::PullRequest {
                base_ref: base.to_string(),
                head_ref: head.to_string(),
            },
            None => GitEvent::Push {
                ref_name: base.to_string(),
            },
        },
        None => {
            return Err(crate::error::Error::GitError(
                "affected requires --base; GitHub event context must be resolved by the action"
                    .into(),
            ))
        }
    };
    let mode = ComparisonMode::from_env();
    let base_commit = resolve_base_commit(&git, &event, mode)?;

    let head_ref = event.head_ref();
    let head_commit = git.resolve_commit(head_ref)?;
    let changed_files = match mode {
        ComparisonMode::MergeBase => git.get_changed_files(&base_commit, Some(head_ref))?,
        ComparisonMode::Tip => git.get_changed_files_from_tip(&base_commit, Some(head_ref))?,
    };

    let workspace = Workspace::discover(config, cwd)?;
    let reasons = explain_affected(&workspace, &changed_files, &config.global_dependencies, cwd);

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
        diagnostics: Some(AffectedDiagnostics {
            comparison: ComparisonDetails {
                mode: match mode {
                    ComparisonMode::MergeBase => "merge-base".into(),
                    ComparisonMode::Tip => "tip".into(),
                },
                requested_base: event.base_ref().into(),
                requested_head: head_ref.into(),
                base_commit,
                head_commit,
            },
            changed_files: relative_paths(&changed_files, cwd),
            reasons,
        }),
    })
}

fn explain_affected(
    workspace: &Workspace,
    changed_files: &[PathBuf],
    global_dependencies: &[String],
    cwd: &Path,
) -> BTreeMap<String, AffectedReason> {
    let global_files =
        crate::workspace::matching_global_files(changed_files, global_dependencies, cwd);
    if !global_files.is_empty() {
        let files = relative_paths(&global_files, cwd);
        return workspace
            .all_projects()
            .iter()
            .map(|project| {
                (
                    project.name.clone(),
                    AffectedReason {
                        kind: "globalDependency".into(),
                        changed_files: files.clone(),
                        dependency_path: vec![],
                    },
                )
            })
            .collect();
    }

    let direct: BTreeMap<String, Vec<String>> = workspace
        .all_projects()
        .iter()
        .filter_map(|project| {
            let files: Vec<PathBuf> = changed_files
                .iter()
                .filter(|file| file.starts_with(&project.path))
                .cloned()
                .collect();
            (!files.is_empty()).then(|| (project.name.clone(), relative_paths(&files, cwd)))
        })
        .collect();
    let direct_names: HashSet<&str> = direct.keys().map(String::as_str).collect();
    let affected = calculate_affected(workspace, changed_files, global_dependencies, cwd, true);

    affected
        .into_iter()
        .filter_map(|project| {
            if let Some(files) = direct.get(&project.name) {
                return Some((
                    project.name,
                    AffectedReason {
                        kind: "direct".into(),
                        changed_files: files.clone(),
                        dependency_path: vec![],
                    },
                ));
            }
            dependency_path(workspace, &project.name, &direct_names).map(|path| {
                (
                    project.name,
                    AffectedReason {
                        kind: "transitiveDependent".into(),
                        changed_files: vec![],
                        dependency_path: path,
                    },
                )
            })
        })
        .collect()
}

fn dependency_path(
    workspace: &Workspace,
    start: &str,
    direct: &HashSet<&str>,
) -> Option<Vec<String>> {
    let mut queue = VecDeque::from([(start.to_string(), vec![start.to_string()])]);
    let mut seen = HashSet::from([start.to_string()]);
    while let Some((name, path)) = queue.pop_front() {
        let mut dependencies = workspace.get_project_by_name(&name)?.dependencies.clone();
        dependencies.sort();
        for dependency in dependencies {
            if workspace.get_project_by_name(&dependency).is_none()
                || !seen.insert(dependency.clone())
            {
                continue;
            }
            let mut next = path.clone();
            next.push(dependency.clone());
            if direct.contains(dependency.as_str()) {
                return Some(next);
            }
            queue.push_back((dependency, next));
        }
    }
    None
}

fn relative_paths(files: &[PathBuf], cwd: &Path) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            file.strip_prefix(cwd)
                .unwrap_or(file)
                .to_string_lossy()
                .into_owned()
        })
        .collect()
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
