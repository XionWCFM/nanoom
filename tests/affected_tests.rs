use nanoom::{
    affected::{
        calculate, generate_matrix, generate_matrix_for_group, AffectedOutput, GroupOutput,
        WorkspaceEntry,
    },
    config::Config,
};
use std::fs;
use tempfile::tempdir;

fn create_package_json(dir: &std::path::Path, name: &str, deps: &[(&str, &str)]) {
    let mut pkg = serde_json::json!({
        "name": name,
        "version": "1.0.0",
        "dependencies": {},
        "devDependencies": {},
        "scripts": {}
    });

    for (dep_name, version) in deps {
        pkg["dependencies"][dep_name] = serde_json::json!(version);
    }

    fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn test_calculate_affected_no_git_env() {
    let dir = tempdir().unwrap();

    let root_pkg = serde_json::json!({
        "name": "root",
        "workspaces": ["packages/*"]
    });
    fs::write(
        dir.path().join("package.json"),
        serde_json::to_string_pretty(&root_pkg).unwrap(),
    )
    .unwrap();

    let pkg_path = dir.path().join("packages/pkg1");
    fs::create_dir_all(&pkg_path).unwrap();
    create_package_json(&pkg_path, "pkg1", &[]);

    let config = Config {
        schema: None,
        group: std::collections::HashMap::new(),
        global_dependencies: vec![],
        workspace: nanoom::config::WorkspaceConfig::default(),
    };

    let result = calculate(&config, dir.path()).await;
    assert!(result.is_err());
}

#[test]
fn test_generate_matrix() {
    let output = AffectedOutput {
        group: std::collections::HashMap::from([
            (
                "ci".to_string(),
                GroupOutput {
                    label: "ci".to_string(),
                    max_parallel: 2,
                    workspaces: vec![
                        WorkspaceEntry {
                            name: "pkg1".to_string(),
                            path: "packages/pkg1".to_string(),
                            task: "test".to_string(),
                            shard: None,
                            isolate: Some(false),
                        },
                        WorkspaceEntry {
                            name: "pkg2".to_string(),
                            path: "packages/pkg2".to_string(),
                            task: "build".to_string(),
                            shard: None,
                            isolate: Some(false),
                        },
                    ],
                },
            ),
            (
                "e2e".to_string(),
                GroupOutput {
                    label: "e2e".to_string(),
                    max_parallel: 4,
                    workspaces: vec![
                        WorkspaceEntry {
                            name: "pkg1".to_string(),
                            path: "packages/pkg1".to_string(),
                            task: "test:e2e".to_string(),
                            shard: Some(1),
                            isolate: Some(false),
                        },
                        WorkspaceEntry {
                            name: "pkg1".to_string(),
                            path: "packages/pkg1".to_string(),
                            task: "test:e2e".to_string(),
                            shard: Some(2),
                            isolate: Some(false),
                        },
                    ],
                },
            ),
        ]),
        has_change: true,
    };

    let matrix = generate_matrix(&output);
    let matrix_obj = matrix.as_object().unwrap();

    assert!(matrix_obj.contains_key("ci"));
    assert!(matrix_obj.contains_key("e2e"));

    let ci_include = matrix_obj["ci"]["include"].as_array().unwrap();
    assert_eq!(ci_include.len(), 2);

    let e2e_include = matrix_obj["e2e"]["include"].as_array().unwrap();
    assert_eq!(e2e_include.len(), 2);

    assert_eq!(e2e_include[0]["shard"], 1);
    assert_eq!(e2e_include[1]["shard"], 2);
}

#[test]
fn test_generate_matrix_for_group() {
    let output = AffectedOutput {
        group: std::collections::HashMap::from([(
            "ci".to_string(),
            GroupOutput {
                label: "ci".to_string(),
                max_parallel: 2,
                workspaces: vec![WorkspaceEntry {
                    name: "pkg1".to_string(),
                    path: "packages/pkg1".to_string(),
                    task: "test".to_string(),
                    shard: None,
                    isolate: Some(false),
                }],
            },
        )]),
        has_change: true,
    };

    let matrix = generate_matrix_for_group(&output, "ci");
    let include = matrix["include"].as_array().unwrap();
    assert_eq!(include.len(), 1);
    assert_eq!(matrix["max_parallel"], 2);
    assert_eq!(include[0]["name"], "pkg1");
    assert_eq!(include[0]["task"], "test");
}

#[test]
fn test_generate_matrix_for_nonexistent_group() {
    let output = AffectedOutput {
        group: std::collections::HashMap::new(),
        has_change: false,
    };

    let matrix = generate_matrix_for_group(&output, "nonexistent");
    let include = matrix["include"].as_array().unwrap();
    assert_eq!(include.len(), 0);
}

#[test]
fn test_workspace_entry_serialization() {
    let entry = WorkspaceEntry {
        name: "pkg1".to_string(),
        path: "packages/pkg1".to_string(),
        task: "test".to_string(),
        shard: Some(1),
        isolate: Some(false),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["name"], "pkg1");
    assert_eq!(json["path"], "packages/pkg1");
    assert_eq!(json["task"], "test");
    assert_eq!(json["shard"], 1);
    assert_eq!(json["isolate"], false);
}

#[test]
fn test_workspace_entry_isolated() {
    let entry = WorkspaceEntry {
        name: "pkg1".to_string(),
        path: "packages/pkg1".to_string(),
        task: "build".to_string(),
        shard: None,
        isolate: Some(true),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["isolate"], true);
    assert!(json["shard"].is_null());
}
