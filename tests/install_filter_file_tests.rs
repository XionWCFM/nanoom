#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn fixture() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), r#"{"name":"root"}"#).unwrap();
    fs::write(
        dir.path().join("nanoom.config.json"),
        r#"{"group":{"ci":{"tasks":["build"]}}}"#,
    )
    .unwrap();
    dir
}

fn fake_manager(dir: &Path, name: &str) -> std::ffi::OsString {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let executable = bin.join(name);
    fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NANOOM_ARGS_LOG\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap()
}

fn run_cli(dir: &Path, args: &[&str], path: &std::ffi::OsStr, log: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nanoom"))
        .current_dir(dir)
        .args(args)
        .env("PATH", path)
        .env("NANOOM_ARGS_LOG", log)
        .output()
        .unwrap()
}

#[test]
fn install_filter_file_runs_the_existing_focused_install_with_deduplicated_filters() {
    let dir = fixture();
    let path = fake_manager(dir.path(), "pnpm");
    let log = dir.path().join("args.log");
    fs::write(
        dir.path().join("filters.json"),
        r#"["pkg-a","pkg-b","pkg-a"]"#,
    )
    .unwrap();

    let output = run_cli(
        dir.path(),
        &[
            "install",
            "--package-manager",
            "pnpm",
            "--filter-file",
            "filters.json",
            "--json",
        ],
        &path,
        &log,
    );
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(log).unwrap(),
        "install\n--frozen-lockfile\n--filter\n.\n--filter\npkg-a...\n--filter\npkg-b...\n"
    );
}

#[test]
fn install_filter_file_rejects_invalid_json_shapes_and_empty_filters_before_execution() {
    let dir = fixture();
    let path = fake_manager(dir.path(), "pnpm");
    let log = dir.path().join("args.log");

    for invalid in [
        "{",
        "{\"filter\":\"pkg-a\"}",
        "[1]",
        "[]",
        "[\"\"]",
        "[\" \"]",
        r#"["line\nbreak"]"#,
        r#"["null\u0000byte"]"#,
    ] {
        fs::write(dir.path().join("filters.json"), invalid).unwrap();
        let output = run_cli(
            dir.path(),
            &[
                "install",
                "--package-manager",
                "pnpm",
                "--filter-file",
                "filters.json",
            ],
            &path,
            &log,
        );
        assert!(
            !output.status.success(),
            "invalid filter file unexpectedly succeeded: {invalid:?}"
        );
        assert!(
            !log.exists(),
            "package manager ran for invalid filter file: {invalid:?}"
        );
    }
}

#[test]
fn install_rejects_filter_and_filter_file_together() {
    let dir = fixture();
    fs::write(dir.path().join("filters.json"), r#"["pkg-a"]"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nanoom"))
        .current_dir(dir.path())
        .args([
            "install",
            "--filter",
            "pkg-b",
            "--filter-file",
            "filters.json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}

#[test]
fn standalone_install_without_filters_still_runs_the_root_install() {
    let dir = fixture();
    let path = fake_manager(dir.path(), "npm");
    let log = dir.path().join("args.log");

    let output = run_cli(
        dir.path(),
        &["install", "--package-manager", "npm", "--json"],
        &path,
        &log,
    );
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(log).unwrap(), "install\n");
}
