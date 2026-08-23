use crate::config::Config;
use crate::error::{Error, Result};
use clap::Args;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Args, Debug, Clone)]
pub struct InstallArgs {
    #[arg(long, help = "Package manager to use (auto, pnpm, yarn, npm)")]
    pub package_manager: Option<String>,

    #[arg(
        long,
        help = "Also run an install in each workspace (root install is always performed for a monorepo)"
    )]
    pub root_install: bool,

    #[arg(long, help = "Install only this workspace and its dependency closure")]
    pub filter: Option<String>,

    #[arg(long, help = "Output a JSON result")]
    pub json: bool,
}

pub async fn execute(args: InstallArgs, config: &Config, base_cwd: &std::path::Path) -> Result<()> {
    let cwd = base_cwd;

    let pm = detect_package_manager(cwd, args.package_manager.as_deref())?;
    eprintln!("Using package manager: {}", pm);

    // Lockfiles belong to the monorepo root for pnpm, yarn, and npm workspaces.
    // Installing independently inside every package is both slower and fails
    // when packages do not have their own lockfile. Always install the root;
    // the opt-in flag retains the legacy per-workspace behavior for projects
    // that explicitly need it.
    if let Some(filter) = &args.filter {
        if pm == "yarn" && is_yarn_berry(cwd) {
            let root = root_workspace_name(cwd)?;
            run_command("yarn", yarn_focused_args(&root, filter), cwd, args.json).await?;
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({"package_manager": pm, "filter": filter, "status": "success"})
                );
            }
            return Ok(());
        }
        if pm == "pnpm" {
            run_command("pnpm", pnpm_focused_args(filter), cwd, args.json).await?;
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({"package_manager": pm, "filter": filter, "status": "success"})
                );
            }
            return Ok(());
        }
        return Err(Error::ConfigValidation(
            "npm focused install is unsupported; use Yarn Berry or pnpm".into(),
        ));
    }
    eprintln!("Installing root dependencies...");
    run_install(&pm, cwd, args.json).await?;

    if !args.root_install {
        return Ok(());
    }

    let workspace = crate::workspace::Workspace::discover(config, cwd)?;

    for project in workspace.all_projects() {
        let project_path = &project.path;
        if project_path.join("package.json").exists() {
            eprintln!("Installing for {}...", project.name);
            run_install(&pm, project_path, args.json).await?;
        }
    }

    if args.json {
        println!(
            "{}",
            serde_json::json!({"package_manager": pm, "status": "success", "root_install": args.root_install})
        );
    }
    Ok(())
}

fn yarn_focused_args(root: &str, filter: &str) -> Vec<String> {
    vec![
        "workspaces".into(),
        "focus".into(),
        root.into(),
        filter.into(),
    ]
}

fn root_workspace_name(dir: &Path) -> Result<String> {
    let content = std::fs::read_to_string(dir.join("package.json")).map_err(|error| {
        Error::ConfigValidation(format!("Cannot read root package.json: {error}"))
    })?;
    let manifest: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| Error::ConfigValidation(format!("Invalid root package.json: {error}")))?;
    manifest
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            Error::ConfigValidation("Root package.json must define a name for Yarn focus".into())
        })
}

fn pnpm_focused_args(filter: &str) -> Vec<String> {
    vec![
        "install".into(),
        "--frozen-lockfile".into(),
        "--filter".into(),
        ".".into(),
        "--filter".into(),
        format!("{filter}..."),
    ]
}

async fn run_command(cmd: &str, args: Vec<String>, dir: &Path, json: bool) -> Result<()> {
    let mut command = Command::new(package_manager_executable(cmd));
    command.current_dir(dir).args(&args);
    let status = if json {
        let output = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        output.status
    } else {
        command.status().await?
    };
    if status.success() {
        Ok(())
    } else {
        Err(Error::CommandFailed {
            command: cmd.into(),
            args,
            code: status.code().unwrap_or(-1),
        })
    }
}

fn package_manager_executable(cmd: &str) -> &str {
    #[cfg(windows)]
    {
        if cmd == "yarn" {
            "yarn.cmd"
        } else {
            cmd
        }
    }
    #[cfg(not(windows))]
    {
        cmd
    }
}

pub fn detect_package_manager(cwd: &Path, explicit: Option<&str>) -> Result<String> {
    if let Some(pm) = explicit {
        if pm != "auto" {
            return Ok(pm.to_string());
        }
    }

    if let Ok(content) = std::fs::read_to_string(cwd.join("package.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(manager) = value.get("packageManager").and_then(|v| v.as_str()) {
                if let Some(name) = manager.split('@').next() {
                    if matches!(name, "pnpm" | "yarn" | "npm") {
                        return Ok(name.to_string());
                    }
                }
            }
        }
    }
    if cwd.join("pnpm-lock.yaml").exists() {
        Ok("pnpm".to_string())
    } else if cwd.join("yarn.lock").exists() {
        Ok("yarn".to_string())
    } else {
        Ok("npm".to_string())
    }
}

async fn run_install(pm: &str, dir: &Path, json: bool) -> Result<()> {
    let (cmd, args): (&str, Vec<String>) = match pm {
        "pnpm" => {
            let mut args = vec!["install".to_string()];
            if dir.join("pnpm-lock.yaml").exists() {
                args.push("--frozen-lockfile".to_string());
            }
            ("pnpm", args)
        }
        "yarn" => {
            let mut args = vec!["install".to_string()];
            if dir.join("yarn.lock").exists() {
                // Yarn Berry renamed `--frozen-lockfile` to `--immutable`.
                // Keep Yarn 1 consumer compatibility while making v4 fixtures
                // and repositories immutable by default.
                args.push(if is_yarn_berry(dir) {
                    "--immutable".to_string()
                } else {
                    "--frozen-lockfile".to_string()
                });
            }
            ("yarn", args)
        }
        "npm" => {
            let command = if dir.join("package-lock.json").exists()
                || dir.join("npm-shrinkwrap.json").exists()
            {
                "ci"
            } else {
                "install"
            };
            ("npm", vec![command.to_string()])
        }
        _ => return Err(Error::PackageManagerNotFound(pm.to_string())),
    };

    #[cfg(windows)]
    let executable = if cmd == "npm" { "npm.cmd" } else { cmd };
    #[cfg(not(windows))]
    let executable = cmd;

    let mut command = Command::new(executable);
    command.current_dir(dir);
    command.args(&args);

    let status = if json {
        let output = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        output.status
    } else {
        command.status().await?
    };

    if !status.success() {
        return Err(Error::CommandFailed {
            command: cmd.to_string(),
            args,
            code: status.code().unwrap_or(-1),
        });
    }

    Ok(())
}

fn is_yarn_berry(dir: &Path) -> bool {
    let config = std::fs::read_to_string(dir.join(".yarnrc.yml")).unwrap_or_default();
    if config.contains("yarnPath:") {
        return true;
    }
    let package_json = std::fs::read_to_string(dir.join("package.json")).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&package_json)
        .ok()
        .and_then(|manifest| manifest.get("packageManager")?.as_str().map(str::to_owned))
        .is_some_and(|manager| manager.starts_with("yarn@") && !manager.starts_with("yarn@1."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(windows))]
    use std::collections::HashMap;

    use tempfile::tempdir;

    #[test]
    fn focused_install_selects_root_and_dependency_closure_without_production_mode() {
        assert_eq!(
            yarn_focused_args("repo-root", "@repo/app"),
            ["workspaces", "focus", "repo-root", "@repo/app"]
        );
        assert_eq!(
            pnpm_focused_args("@repo/app"),
            [
                "install",
                "--frozen-lockfile",
                "--filter",
                ".",
                "--filter",
                "@repo/app..."
            ]
        );
    }

    #[test]
    fn test_detect_package_manager_explicit() {
        let dir = tempdir().unwrap();
        assert_eq!(
            detect_package_manager(dir.path(), Some("pnpm")).unwrap(),
            "pnpm"
        );
        assert_eq!(
            detect_package_manager(dir.path(), Some("yarn")).unwrap(),
            "yarn"
        );
        assert_eq!(
            detect_package_manager(dir.path(), Some("npm")).unwrap(),
            "npm"
        );
        assert_eq!(
            detect_package_manager(dir.path(), Some("auto")).unwrap(),
            "npm"
        );
    }

    #[test]
    fn test_detect_package_manager_lockfiles() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(detect_package_manager(dir.path(), None).unwrap(), "pnpm");

        let dir2 = tempdir().unwrap();
        std::fs::write(dir2.path().join("yarn.lock"), "").unwrap();
        assert_eq!(detect_package_manager(dir2.path(), None).unwrap(), "yarn");

        let dir3 = tempdir().unwrap();
        std::fs::write(dir3.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_package_manager(dir3.path(), None).unwrap(), "npm");
    }

    #[test]
    fn detects_yarn_berry_from_manifest_or_configuration() {
        let manifest = tempdir().unwrap();
        std::fs::write(
            manifest.path().join("package.json"),
            r#"{"packageManager":"yarn@4.9.1"}"#,
        )
        .unwrap();
        assert!(is_yarn_berry(manifest.path()));

        let config = tempdir().unwrap();
        std::fs::write(
            config.path().join(".yarnrc.yml"),
            "yarnPath: .yarn/releases/yarn.cjs\n",
        )
        .unwrap();
        assert!(is_yarn_berry(config.path()));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_run_install_yarn_berry() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
        let result = run_install("yarn", dir.path(), false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_install_unknown_package_manager() {
        let dir = tempdir().unwrap();
        let result = run_install("bun", dir.path(), false).await;
        assert!(matches!(result, Err(Error::PackageManagerNotFound(pm)) if pm == "bun"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_run_install_command_failure() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"test","dependencies":{"nanoom-test-package-that-does-not-exist":"1.0.0"}}"#,
        )
        .unwrap();
        // An invalid lockfile forces the frozen install path to fail.
        std::fs::write(dir.path().join("yarn.lock"), "\n").unwrap();
        let result = run_install("yarn", dir.path(), false).await;
        match result {
            Err(Error::CommandFailed { command, code, .. }) => {
                assert_eq!(command, "yarn");
                assert_ne!(code, 0);
            }
            other => panic!("expected CommandFailed, got {:?}", other),
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_execute_installs_root_and_projects() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","private":true}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
        std::fs::write(
            dir.path().join("packages/app/package.json"),
            r#"{"name":"app"}"#,
        )
        .unwrap();

        let mut group = HashMap::new();
        group.insert(
            "ci".to_string(),
            crate::config::GroupConfig {
                tasks: vec!["build".to_string()],
                rules: vec![],
            },
        );
        let config = Config {
            schema: None,
            group,
            global_dependencies: vec![],
            workspace: crate::config::WorkspaceConfig {
                include: vec!["packages/*".to_string()],
                exclude: vec![],
            },
        };

        let args = InstallArgs {
            package_manager: Some("yarn".to_string()),
            root_install: true,
            filter: None,
            json: false,
        };
        execute(args, &config, dir.path()).await.unwrap();
    }
}
