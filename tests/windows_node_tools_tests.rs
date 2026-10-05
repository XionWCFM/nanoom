#![cfg(windows)]

use std::{fs, path::Path, process::Command};
use tempfile::{Builder, TempDir};

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn fixture() -> TempDir {
    let dir = Builder::new().prefix("nanoom tools ").tempdir().unwrap();
    write(
        dir.path(),
        "package.json",
        r#"{"name":"root","private":true,"workspaces":["packages/*"]}"#,
    );
    write(
        dir.path(),
        "packages/app/package.json",
        r#"{"name":"app","scripts":{"test":"echo test"}}"#,
    );
    write(
        dir.path(),
        "nanoom.config.json",
        r#"{"group":{"ci":{"tasks":["test"]}}}"#,
    );
    dir
}

fn assert_cli(mut command: Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "success");
    result
}

#[test]
fn installed_windows_turbo_and_nx_execute_without_global_shims() {
    let dir = fixture();
    let root = dir.path();
    let log = root.join("tasks.log");
    for runner in ["turbo", "nx"] {
        write(root, &format!("{runner}.json"), "{}");
        write(
            root,
            &format!("node_modules/.bin/{runner}.cmd"),
            "@echo off\r\necho %* >> \"%NANOOM_ARGS_LOG%\"\r\nexit /b 0\r\n",
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_nanoom"));
        command
            .arg("-C")
            .arg(root)
            .args(["run", "ci", "test", "--all", "--json"])
            .env("NANOOM_ARGS_LOG", &log);
        let result = assert_cli(command);
        assert_eq!(result["executions"][0]["runner"], runner);
        fs::remove_file(root.join(format!("{runner}.json"))).unwrap();
    }
    let calls = fs::read_to_string(log).unwrap().replace('"', "");
    assert!(calls.contains("run test --filter app"), "{calls}");
    assert!(calls.contains("run app:test"), "{calls}");
}

#[test]
fn windows_package_managers_execute_focused_and_full_installs() {
    let dir = fixture();
    let root = dir.path();
    let bin = root.join("bin");
    let log = root.join("install.log");
    for pm in ["pnpm", "yarn", "npm"] {
        write(
            root,
            &format!("bin/{pm}.cmd"),
            &format!("@echo off\r\necho {pm} %* >> \"%NANOOM_ARGS_LOG%\"\r\nexit /b 0\r\n"),
        );
    }
    let old_path = std::env::var_os("PATH").unwrap();
    let path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&old_path))).unwrap();
    for (pm, version) in [("pnpm", "10.12.4"), ("yarn", "4.11.0"), ("npm", "11.0.0")] {
        write(
            root,
            "package.json",
            &format!(
                r#"{{"name":"root","private":true,"workspaces":["packages/*"],"packageManager":"{pm}@{version}"}}"#
            ),
        );
        for focused in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_nanoom"));
            command
                .arg("-C")
                .arg(root)
                .args(["install", "--json"])
                .env("PATH", &path)
                .env("NANOOM_ARGS_LOG", &log);
            if focused {
                command.args(["--filter", "app"]);
            }
            assert_cli(command);
        }
    }
    let calls = fs::read_to_string(log).unwrap().replace('"', "");
    assert!(
        calls.contains("pnpm install --frozen-lockfile --prod=false --filter . --filter app..."),
        "{calls}"
    );
    assert!(calls.contains("yarn workspaces focus root app"), "{calls}");
    assert!(calls.contains("npm install"), "{calls}");
    assert!(
        calls.contains("npm ci --include-workspace-root --include=dev --workspace app"),
        "{calls}"
    );
}
