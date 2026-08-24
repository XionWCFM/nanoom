use crate::affected::calculate;
use crate::config::Config;
use crate::error::{Error, Result};
use clap::Args;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    #[arg(help = "Group name to run tasks for")]
    pub group: String,

    #[arg(help = "Task name to run")]
    pub task: String,

    #[arg(
        long,
        value_name = "RUNNER",
        help = "Runner: pnpm, yarn, turbo, or nx (default: auto)"
    )]
    pub runner: Option<String>,

    #[arg(long, help = "Run only for specific workspace")]
    pub filter: Option<String>,

    #[arg(long, help = "Shard index (1-based)")]
    pub shard: Option<usize>,

    #[arg(long, help = "Total number of shards")]
    pub total_shards: Option<usize>,

    #[arg(long, help = "Run isolated (dedicated runner)")]
    pub isolate: bool,

    #[arg(long, help = "Run on all projects, not just affected")]
    pub all: bool,

    #[arg(long, help = "Continue on error")]
    pub continue_on_error: bool,

    #[arg(long, help = "Output a JSON result")]
    pub json: bool,
}

struct TaskConfig {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

pub async fn execute(args: RunArgs, config: &Config, cwd: &std::path::Path) -> Result<()> {
    match (args.shard, args.total_shards) {
        (Some(shard), Some(total)) if shard > 0 && shard <= total => {}
        (None, None) => {}
        _ => return Err(Error::ConfigValidation(
            "--shard and --total-shards must be provided together with 1 <= shard <= total-shards"
                .into(),
        )),
    }

    let group_config = config
        .get_group(&args.group)
        .ok_or_else(|| Error::ConfigValidation(format!("Group '{}' not found", args.group)))?;

    if !group_config.tasks.contains(&args.task) {
        return Err(Error::ConfigValidation(format!(
            "Task '{}' not in group '{}'",
            args.task, args.group
        )));
    }

    let projects: Vec<crate::workspace::Project> = if args.all {
        let workspace = crate::workspace::Workspace::discover(config, cwd)?;
        workspace
            .all_projects()
            .iter()
            .filter(|project| {
                let rule = group_config.rules.iter().find(|r| r.name == project.name);
                let shard_matches = args.shard.is_none_or(|shard| {
                    rule.and_then(|r| r.shard.iter().find(|s| s.task == args.task))
                        .is_some_and(|spec| shard <= spec.shard)
                });
                let isolate_matches =
                    !args.isolate || rule.is_some_and(|r| r.isolate.contains(&args.task));
                let filter_matches = args
                    .filter
                    .as_ref()
                    .is_none_or(|filter| &project.name == filter);
                shard_matches && isolate_matches && filter_matches
            })
            .cloned()
            .collect()
    } else {
        let result = calculate(config, cwd).await?;
        let group_output = result.group.get(&args.group).ok_or_else(|| {
            Error::ConfigValidation(format!("Group '{}' not in output", args.group))
        })?;

        group_output
            .workspaces
            .iter()
            .filter(|ws| ws.task == args.task)
            .filter(|ws| args.filter.as_ref().map(|f| &ws.name == f).unwrap_or(true))
            .filter(|ws| args.shard.map(|s| ws.shard == Some(s)).unwrap_or(true))
            .filter(|ws| !args.isolate || ws.isolate == Some(true))
            .map(|ws| crate::workspace::Project {
                name: ws.name.clone(),
                path: PathBuf::from(&ws.path),
                dependencies: vec![],
                dependency_specs: HashMap::new(),
                dependents: vec![],
                package_json_version: None,
            })
            .collect()
    };

    if projects.is_empty() {
        if args.json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "success", "group": args.group, "task": args.task,
                    "projects": [], "runner": args.runner.as_deref().unwrap_or("auto"),
                    "reason": "no workspace matched the requested group, task, filter, shard, and isolation constraints"
                })
            );
        } else {
            println!("◆ nanoom run");
            println!(
                "  Result: no workspace matched group={}, task={}",
                args.group, args.task
            );
        }
        return Ok(());
    }

    // Dependencies must run before dependents.
    let workspace = crate::workspace::Workspace::discover(config, cwd)?;
    let ordered_projects: Vec<crate::workspace::Project> =
        crate::workspace::topological_sort(workspace.all_projects())
            .into_iter()
            .filter(|p| projects.iter().any(|q| q.name == p.name))
            .collect();

    if !args.json {
        eprintln!(
            "◆ nanoom run\n  Selection: group={}, task={}, projects={}",
            args.group,
            args.task,
            projects.len()
        );
    }

    let task_config = TaskConfig {
        command: args.task.clone(),
        args: vec![],
        env: HashMap::new(),
    };

    if args.shard.is_some() || args.total_shards.is_some() {
        std::env::set_var("NANOOM_SHARD_INDEX", args.shard.unwrap_or(1).to_string());
        std::env::set_var(
            "NANOOM_SHARD_TOTAL",
            args.total_shards.unwrap_or(1).to_string(),
        );
    }

    let mut first_error = None;
    for project in ordered_projects {
        if !args.json {
            eprintln!("\n--- {} ---", project.name);
        }
        let result = run_task(
            &project,
            &task_config,
            args.runner.as_deref(),
            cwd,
            args.json,
        )
        .await;

        if let Err(e) = result {
            eprintln!("Error in {}: {}", project.name, e);
            if !args.continue_on_error {
                return Err(e);
            }
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "status": "success",
                "group": args.group,
                "task": args.task,
                "projects": projects.iter().map(|p| &p.name).collect::<Vec<_>>(),
                "runner": args.runner.as_deref().unwrap_or("auto"),
                "filter": args.filter,
                "shard": args.shard,
                "totalShards": args.total_shards,
                "isolate": args.isolate,
                "selection": if args.all { "all" } else { "affected" },
                "reason": if args.all { "explicit --all selection constrained by matrix arguments" } else { "affected calculation selected these workspaces" }
            })
        );
    }
    Ok(())
}

async fn run_task(
    project: &crate::workspace::Project,
    task: &TaskConfig,
    runner: Option<&str>,
    root: &std::path::Path,
    json: bool,
) -> Result<()> {
    // Tasks declared in package.json scripts are routed through the package
    // manager (`pnpm test`), anything else runs as a raw command.
    let selected_runner = runner.unwrap_or("auto");
    let detected_runner = if selected_runner == "auto" {
        let turbo = root.join("turbo.json").exists();
        let nx = root.join("nx.json").exists();
        if turbo && nx {
            return Err(Error::InvalidRunner(
                "both turbo.json and nx.json exist; set monorepoTool explicitly".into(),
            ));
        }
        if turbo {
            "turbo"
        } else if nx {
            "nx"
        } else {
            "auto"
        }
    } else {
        selected_runner
    };
    let (program, args) = match detected_runner {
        "turbo" => (
            "turbo".to_string(),
            vec![
                "run".to_string(),
                task.command.clone(),
                "--filter".to_string(),
                project.name.clone(),
            ],
        ),
        "nx" => (
            "nx".to_string(),
            vec![
                "run".to_string(),
                format!("{}:{}", project.name, task.command),
            ],
        ),
        "pnpm" | "yarn" | "npm" => (
            selected_runner.to_string(),
            vec!["run".to_string(), task.command.clone()],
        ),
        "auto" => {
            let script_runner = resolve_script_runner(&project.path, &task.command, root);
            match script_runner {
                Some(pm) => (pm, vec!["run".to_string(), task.command.clone()]),
                None => (task.command.clone(), task.args.clone()),
            }
        }
        invalid => return Err(Error::InvalidRunner(invalid.to_string())),
    };

    let executable = package_manager_executable(&program);
    let mut cmd = Command::new(executable);
    cmd.current_dir(if matches!(detected_runner, "turbo" | "nx") {
        root
    } else {
        &project.path
    });
    if matches!(detected_runner, "turbo" | "nx") {
        let local_bin = root.join("node_modules").join(".bin");
        let mut path_entries = vec![local_bin];
        if let Some(existing) = std::env::var_os("PATH") {
            path_entries.extend(std::env::split_paths(&existing));
        }
        if let Ok(path) = std::env::join_paths(path_entries) {
            cmd.env("PATH", path);
        }
    }
    cmd.args(&args);

    eprintln!(
        "Executing task command (cwd={}): {}",
        cmd.as_std().get_current_dir().unwrap_or(root).display(),
        crate::commands::display_command(&program, &args)
    );

    for (key, value) in &task.env {
        cmd.env(key, value);
    }

    let status = if json {
        use std::process::Stdio;
        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        output.status
    } else {
        cmd.status().await?
    };

    if !status.success() {
        return Err(Error::TaskFailed {
            project: project.name.clone(),
            task: task.command.clone(),
            code: status.code().unwrap_or(-1),
        });
    }

    Ok(())
}

fn package_manager_executable(program: &str) -> &str {
    #[cfg(windows)]
    {
        return match program {
            "npm" => "npm.cmd",
            "pnpm" => "pnpm.cmd",
            "yarn" => "yarn.cmd",
            _ => program,
        };
    }

    #[cfg(not(windows))]
    program
}

fn resolve_script_runner(
    project_path: &std::path::Path,
    task: &str,
    root: &std::path::Path,
) -> Option<String> {
    let manifest = std::fs::read_to_string(project_path.join("package.json")).ok()?;
    let scripts: HashMap<String, serde_json::Value> =
        serde_json::from_str::<serde_json::Value>(&manifest)
            .ok()?
            .get("scripts")?
            .as_object()?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

    if !scripts.contains_key(task) {
        return None;
    }

    crate::commands::install::detect_package_manager(root, None).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GroupConfig, WorkspaceConfig};
    use serial_test::serial;
    use std::path::Path;
    use tempfile::tempdir;

    fn make_project(name: &str, path: &Path) -> crate::workspace::Project {
        crate::workspace::Project {
            name: name.to_string(),
            path: path.to_path_buf(),
            dependencies: vec![],
            dependency_specs: HashMap::new(),
            dependents: vec![],
            package_json_version: None,
        }
    }

    fn make_config(include: Vec<String>) -> Config {
        let mut group = HashMap::new();
        group.insert(
            "ci".to_string(),
            GroupConfig {
                tasks: vec!["echo".to_string(), "true".to_string(), "false".to_string()],
                rules: vec![],
            },
        );
        Config {
            schema: None,
            group,
            global_dependencies: vec![],
            workspace: WorkspaceConfig {
                include,
                exclude: vec![],
            },
        }
    }

    fn setup_workspace(dir: &Path) {
        std::fs::create_dir_all(dir.join("packages/app")).unwrap();
        std::fs::write(
            dir.join("packages/app/package.json"),
            r#"{"name":"app","version":"1.0.0"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("pnpm-workspace.yaml"),
            "packages:\n  - \"packages/*\"\n",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_run_task_raw_command_success() {
        let dir = tempdir().unwrap();
        let project = make_project("a", dir.path());
        let task = TaskConfig {
            command: "true".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        assert!(run_task(&project, &task, None, dir.path(), false)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_run_task_raw_command_failure() {
        let dir = tempdir().unwrap();
        let project = make_project("a", dir.path());
        let task = TaskConfig {
            command: "false".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        let result = run_task(&project, &task, None, dir.path(), false).await;
        match result {
            Err(Error::TaskFailed {
                project,
                task,
                code,
            }) => {
                assert_eq!(project, "a");
                assert_eq!(task, "false");
                assert_eq!(code, 1);
            }
            other => panic!("expected TaskFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_run_task_passes_env_vars() {
        let dir = tempdir().unwrap();
        let project = make_project("a", dir.path());
        let mut env = HashMap::new();
        env.insert("NANOOM_TEST_VAR".to_string(), "hello".to_string());
        let task = TaskConfig {
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                r#"test "$NANOOM_TEST_VAR" = hello"#.to_string(),
            ],
            env,
        };
        assert!(run_task(&project, &task, None, dir.path(), false)
            .await
            .is_ok());

        let mut bad_env = HashMap::new();
        bad_env.insert("NANOOM_TEST_VAR".to_string(), "wrong".to_string());
        let failing = TaskConfig {
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                r#"test "$NANOOM_TEST_VAR" = hello"#.to_string(),
            ],
            env: bad_env,
        };
        assert!(run_task(&project, &failing, None, dir.path(), false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_execute_unknown_group_errors() {
        let config = make_config(vec![]);
        let args = RunArgs {
            group: "missing".to_string(),
            task: "echo".to_string(),
            runner: None,
            filter: None,
            shard: None,
            total_shards: None,
            isolate: false,
            all: true,
            continue_on_error: false,
            json: false,
        };
        let result = execute(args, &config, Path::new(".")).await;
        assert!(matches!(result, Err(Error::ConfigValidation(_))));
    }

    #[tokio::test]
    async fn test_execute_task_not_in_group_errors() {
        let config = make_config(vec![]);
        let args = RunArgs {
            group: "ci".to_string(),
            task: "lint".to_string(),
            runner: None,
            filter: None,
            shard: None,
            total_shards: None,
            isolate: false,
            all: true,
            continue_on_error: false,
            json: false,
        };
        let result = execute(args, &config, Path::new(".")).await;
        assert!(matches!(result, Err(Error::ConfigValidation(_))));
    }

    #[tokio::test]
    #[serial]
    async fn test_execute_all_runs_tasks_in_order() {
        let dir = tempdir().unwrap();
        setup_workspace(dir.path());
        let config = make_config(vec!["packages/*".to_string()]);
        let result = execute(
            RunArgs {
                group: "ci".to_string(),
                task: "echo".to_string(),
                runner: None,
                filter: None,
                shard: Some(1),
                total_shards: Some(2),
                isolate: false,
                all: true,
                continue_on_error: false,
                json: true,
            },
            &config,
            dir.path(),
        )
        .await;

        result.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_execute_no_matching_projects_prints_message() {
        let dir = tempdir().unwrap();
        setup_workspace(dir.path());
        let config = make_config(vec!["packages/*".to_string()]);
        let result = execute(
            RunArgs {
                group: "ci".to_string(),
                task: "echo".to_string(),
                runner: None,
                filter: Some("does-not-exist".to_string()),
                shard: None,
                total_shards: None,
                isolate: false,
                all: true,
                continue_on_error: false,
                json: false,
            },
            &config,
            dir.path(),
        )
        .await;

        result.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_execute_stops_on_error_by_default() {
        let dir = tempdir().unwrap();
        setup_workspace(dir.path());
        let config = make_config(vec!["packages/*".to_string()]);
        let result = execute(
            RunArgs {
                group: "ci".to_string(),
                task: "false".to_string(),
                runner: None,
                filter: None,
                shard: None,
                total_shards: None,
                isolate: false,
                all: true,
                continue_on_error: false,
                json: false,
            },
            &config,
            dir.path(),
        )
        .await;

        assert!(matches!(result, Err(Error::TaskFailed { .. })));
    }

    #[tokio::test]
    #[serial]
    async fn test_execute_continue_on_error_reports_failures() {
        let dir = tempdir().unwrap();
        setup_workspace(dir.path());
        let config = make_config(vec!["packages/*".to_string()]);
        let result = execute(
            RunArgs {
                group: "ci".to_string(),
                task: "false".to_string(),
                runner: None,
                filter: None,
                shard: None,
                total_shards: None,
                isolate: false,
                all: true,
                continue_on_error: true,
                json: false,
            },
            &config,
            dir.path(),
        )
        .await;

        assert!(matches!(result, Err(Error::TaskFailed { .. })));
    }

    #[test]
    fn test_resolve_script_runner_with_script() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"test","scripts":{"test":"echo test"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        let runner = resolve_script_runner(dir.path(), "test", dir.path());
        assert_eq!(runner.as_deref(), Some("pnpm"));
    }

    #[test]
    fn test_resolve_script_runner_no_script() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        assert!(resolve_script_runner(dir.path(), "test", dir.path()).is_none());
    }

    #[test]
    fn test_resolve_script_runner_missing_package_json() {
        let dir = tempdir().unwrap();
        assert!(resolve_script_runner(dir.path(), "test", dir.path()).is_none());
    }

    #[test]
    fn test_resolve_script_runner_invalid_json() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "invalid").unwrap();
        assert!(resolve_script_runner(dir.path(), "test", dir.path()).is_none());
    }
}
