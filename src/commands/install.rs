use crate::config::Config;
use crate::error::{Error, Result};
use clap::Args;
use std::path::Path;
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
}

pub async fn execute(args: InstallArgs, config: &Config, base_cwd: &std::path::Path) -> Result<()> {
    let cwd = base_cwd;

    let pm = detect_package_manager(cwd, args.package_manager.as_deref())?;
    println!("Using package manager: {}", pm);

    // Lockfiles belong to the monorepo root for pnpm, yarn, and npm workspaces.
    // Installing independently inside every package is both slower and fails
    // when packages do not have their own lockfile. Always install the root;
    // the opt-in flag retains the legacy per-workspace behavior for projects
    // that explicitly need it.
    println!("Installing root dependencies...");
    run_install(&pm, cwd).await?;

    if !args.root_install {
        return Ok(());
    }

    let workspace = crate::workspace::Workspace::discover(config, cwd)?;

    for project in workspace.all_projects() {
        let project_path = &project.path;
        if project_path.join("package.json").exists() {
            println!("Installing for {}...", project.name);
            run_install(&pm, project_path).await?;
        }
    }

    Ok(())
}

pub fn detect_package_manager(cwd: &Path, explicit: Option<&str>) -> Result<String> {
    if let Some(pm) = explicit {
        if pm != "auto" {
            return Ok(pm.to_string());
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

async fn run_install(pm: &str, dir: &Path) -> Result<()> {
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
                args.push("--frozen-lockfile".to_string());
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

    let mut command = Command::new(cmd);
    command.current_dir(dir);
    command.args(&args);

    let status = command.status().await?;

    if !status.success() {
        return Err(Error::CommandFailed {
            command: cmd.to_string(),
            args,
            code: status.code().unwrap_or(-1),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use tempfile::tempdir;

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

    #[tokio::test]
    async fn test_run_install_npm() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();
        let result = run_install("npm", dir.path()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_install_unknown_package_manager() {
        let dir = tempdir().unwrap();
        let result = run_install("bun", dir.path()).await;
        assert!(matches!(result, Err(Error::PackageManagerNotFound(pm)) if pm == "bun"));
    }

    #[tokio::test]
    async fn test_run_install_command_failure() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"test","dependencies":{"nanoom-test-package-that-does-not-exist":"1.0.0"}}"#,
        )
        .unwrap();
        // An invalid lockfile forces the frozen install path to fail.
        std::fs::write(dir.path().join("package-lock.json"), "{}\n").unwrap();
        let result = run_install("npm", dir.path()).await;
        match result {
            Err(Error::CommandFailed { command, code, .. }) => {
                assert_eq!(command, "npm");
                assert_ne!(code, 0);
            }
            other => panic!("expected CommandFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_installs_root_and_projects() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","private":true}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
        std::fs::write(
            dir.path().join("packages/app/package.json"),
            r#"{"name":"app"}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("packages/app/package-lock.json"), "{}").unwrap();

        let mut group = HashMap::new();
        group.insert(
            "ci".to_string(),
            crate::config::GroupConfig {
                tasks: vec!["build".to_string()],
                concurrency: 1,
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
            package_manager: Some("npm".to_string()),
            root_install: true,
        };
        execute(args, &config, dir.path()).await.unwrap();
    }
}
