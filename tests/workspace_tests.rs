use nanoom::config::{Config, WorkspaceConfig};
use nanoom::workspace::{apply_rules, calculate_affected, Workspace};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_json(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn package_json(name: &str, deps: &[(&str, &str)]) -> serde_json::Value {
    let mut dependencies = serde_json::Map::new();
    for (dep_name, version) in deps {
        dependencies.insert(dep_name.to_string(), serde_json::json!(version));
    }

    serde_json::json!({
        "name": name,
        "version": "1.0.0",
        "scripts": { "test": "echo test" },
        "dependencies": dependencies
    })
}

#[test]
fn test_workspace_protocol_links_propagate() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());

    // app links lib through the workspace protocol.
    write_json(
        &dir.path().join("packages/app/package.json"),
        &serde_json::json!({
            "name": "app",
            "version": "1.0.0",
            "dependencies": { "lib": "workspace:*" }
        }),
    );
    write_json(
        &dir.path().join("packages/lib/package.json"),
        &serde_json::json!({ "name": "lib", "version": "1.0.0" }),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert!(workspace
        .get_project_by_name("lib")
        .unwrap()
        .dependents
        .contains(&"app".to_string()));

    let changed = vec![dir.path().join("packages/lib/src/x.ts")];
    let affected = calculate_affected(&workspace, &changed, &[], dir.path(), true);
    let names: Vec<&str> = affected.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["app", "lib"]);
}

#[test]
fn test_peer_dependencies_propagate() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());

    write_json(
        &dir.path().join("packages/plugin/package.json"),
        &serde_json::json!({
            "name": "plugin",
            "version": "1.0.0",
            "peerDependencies": { "core": "^1.0.0" }
        }),
    );
    write_json(
        &dir.path().join("packages/core/package.json"),
        &serde_json::json!({ "name": "core", "version": "1.4.2" }),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert!(workspace
        .get_project_by_name("core")
        .unwrap()
        .dependents
        .contains(&"plugin".to_string()));
}

#[test]
fn test_incompatible_registry_range_is_not_an_edge() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());

    // `^2.0.0` cannot resolve to the local 1.x package — this dependency
    // comes from the registry and must NOT create a graph edge.
    write_json(
        &dir.path().join("packages/app/package.json"),
        &package_json("app", &[("lib", "^2.0.0")]),
    );
    write_json(
        &dir.path().join("packages/lib/package.json"),
        &serde_json::json!({ "name": "lib", "version": "1.9.0" }),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert!(workspace
        .get_project_by_name("lib")
        .unwrap()
        .dependents
        .is_empty());
}

#[test]
fn test_compatible_semver_range_is_an_edge() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());

    // npm-style workspaces link same-name packages when the range resolves.
    write_json(
        &dir.path().join("packages/app/package.json"),
        &package_json("app", &[("lib", "^1.0.0")]),
    );
    write_json(
        &dir.path().join("packages/lib/package.json"),
        &serde_json::json!({ "name": "lib", "version": "1.3.7" }),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert!(workspace
        .get_project_by_name("lib")
        .unwrap()
        .dependents
        .contains(&"app".to_string()));
}

fn yarn_workspace_fixture(dir: &Path) {
    write_json(
        &dir.join("package.json"),
        &serde_json::json!({
            "name": "root",
            "private": true,
            "workspaces": ["packages/*"]
        }),
    );
}

fn simple_config(include: &[&str], exclude: &[&str]) -> Config {
    Config {
        schema: None,
        group: std::collections::HashMap::new(),
        global_dependencies: vec![],
        workspace: WorkspaceConfig {
            include: include.iter().map(|s| s.to_string()).collect(),
            exclude: exclude.iter().map(|s| s.to_string()).collect(),
        },
    }
}

#[test]
fn test_discover_no_workspace_files() {
    let dir = tempdir().unwrap();
    let config = simple_config(&["packages/*"], &[]);

    let workspace = Workspace::discover(&config, dir.path()).unwrap();
    assert_eq!(workspace.project_count(), 0);
}

#[test]
fn test_discover_yarn_workspaces_array_form() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/mypkg/package.json"),
        &package_json("mypkg", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert_eq!(workspace.project_count(), 1);
    let pkg = workspace.get_project_by_name("mypkg").unwrap();
    assert_eq!(pkg.name, "mypkg");
}

#[test]
fn duplicate_workspace_names_are_rejected() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/a/package.json"),
        &package_json("duplicate", &[]),
    );
    write_json(
        &dir.path().join("packages/b/package.json"),
        &package_json("duplicate", &[]),
    );

    let error = Workspace::discover(&simple_config(&["packages/*"], &[]), dir.path())
        .err()
        .expect("duplicate package names must not overwrite graph nodes");
    assert!(error
        .to_string()
        .contains("Duplicate workspace name 'duplicate'"));
}

#[test]
fn test_discover_yarn_workspaces_object_form() {
    let dir = tempdir().unwrap();
    write_json(
        &dir.path().join("package.json"),
        &serde_json::json!({
            "name": "root",
            "workspaces": { "packages": ["packages/*"] }
        }),
    );
    write_json(
        &dir.path().join("packages/mypkg/package.json"),
        &package_json("mypkg", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert!(workspace.get_project_by_name("mypkg").is_some());
}

#[test]
fn test_discover_pnpm_workspace() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    )
    .unwrap();
    write_json(
        &dir.path().join("packages/mypkg/package.json"),
        &package_json("mypkg", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert!(workspace.get_project_by_name("mypkg").is_some());
}

#[test]
fn test_discover_nx_workspace() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("nx.json"), r#"{"namedInputs": {}}"#).unwrap();
    write_json(
        &dir.path().join("apps/myapp/project.json"),
        &serde_json::json!({ "name": "myapp" }),
    );
    write_json(
        &dir.path().join("apps/myapp/package.json"),
        &package_json("myapp", &[]),
    );

    let config = simple_config(&["apps/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert_eq!(workspace.project_count(), 1);
    assert_eq!(workspace.all_projects()[0].name, "myapp");
}

#[test]
fn test_turbo_uses_package_manager_workspace_scope() {
    let dir = tempdir().unwrap();
    write_json(
        &dir.path().join("package.json"),
        &serde_json::json!({"workspaces": ["packages/*"]}),
    );
    fs::write(dir.path().join("turbo.json"), "{\"pipeline\": {}}\n").unwrap();
    write_json(
        &dir.path().join("packages/app/package.json"),
        &package_json("app", &[]),
    );
    write_json(
        &dir.path().join("tools/unrelated/package.json"),
        &package_json("unrelated", &[]),
    );

    let config = simple_config(&["**"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();
    assert!(workspace.get_project_by_name("app").is_some());
    assert!(workspace.get_project_by_name("unrelated").is_none());
}

#[test]
fn test_nx_explicit_projects_scope() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("nx.json"),
        r#"{"projects":{"app":"apps/app"}}"#,
    )
    .unwrap();
    write_json(
        &dir.path().join("apps/app/package.json"),
        &package_json("app", &[]),
    );
    write_json(
        &dir.path().join("packages/unrelated/package.json"),
        &package_json("unrelated", &[]),
    );

    let config = simple_config(&["**"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();
    assert!(workspace.get_project_by_name("app").is_some());
    assert!(workspace.get_project_by_name("unrelated").is_none());
}

#[test]
fn test_workspace_include_exclude_override() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());

    // Root declares ["*"] so both dirs are candidates; config narrows it down.
    write_json(
        &dir.path().join("package.json"),
        &serde_json::json!({ "name": "root", "workspaces": ["*"] }),
    );
    write_json(
        &dir.path().join("excluded/package.json"),
        &package_json("excluded", &[]),
    );
    write_json(
        &dir.path().join("included/package.json"),
        &package_json("included", &[]),
    );

    let config = simple_config(&["included"], &["excluded"]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    assert!(workspace.get_project_by_name("included").is_some());
    assert!(workspace.get_project_by_name("excluded").is_none());
}

#[test]
fn test_project_without_name_falls_back_to_dir_name() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/unnamed/package.json"),
        &serde_json::json!({ "version": "1.0.0" }),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    let pkg = workspace.get_project_by_name("unnamed");
    assert!(pkg.is_some());
}

#[test]
fn test_build_dependents() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/app/package.json"),
        &package_json("app", &[("lib", "^1.0.0")]),
    );
    write_json(
        &dir.path().join("packages/lib/package.json"),
        &package_json("lib", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    let lib = workspace.get_project_by_name("lib").unwrap();
    assert!(lib.dependents.contains(&"app".to_string()));

    let app = workspace.get_project_by_name("app").unwrap();
    assert!(app.dependencies.contains(&"lib".to_string()));
    assert!(app.dependents.is_empty());
}

#[test]
fn test_calculate_affected_no_changes() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/pkg1/package.json"),
        &package_json("pkg1", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    let affected = calculate_affected(&workspace, &[], &[], dir.path(), true);
    assert!(affected.is_empty());
}

#[test]
fn test_calculate_affected_direct_change() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/pkg1/package.json"),
        &package_json("pkg1", &[]),
    );
    write_json(
        &dir.path().join("packages/pkg2/package.json"),
        &package_json("pkg2", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    let changed = vec![dir.path().join("packages/pkg1/src/index.ts")];
    let affected = calculate_affected(&workspace, &changed, &[], dir.path(), false);

    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0].name, "pkg1");
}

#[test]
fn test_calculate_affected_includes_dependents() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/lib/package.json"),
        &package_json("lib", &[]),
    );
    write_json(
        &dir.path().join("packages/app/package.json"),
        &package_json("app", &[("lib", "^1.0.0")]),
    );
    write_json(
        &dir.path().join("packages/unrelated/package.json"),
        &package_json("unrelated", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    let changed = vec![dir.path().join("packages/lib/src/lib.ts")];

    let without_dependents = calculate_affected(&workspace, &changed, &[], dir.path(), false);
    assert_eq!(without_dependents.len(), 1);

    let with_dependents = calculate_affected(&workspace, &changed, &[], dir.path(), true);
    let names: Vec<&str> = with_dependents.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"lib"));
    assert!(names.contains(&"app"));
    assert!(!names.contains(&"unrelated"));
}

#[test]
fn test_calculate_affected_global_dependency_triggers_all() {
    let dir = tempdir().unwrap();
    yarn_workspace_fixture(dir.path());
    write_json(
        &dir.path().join("packages/pkg1/package.json"),
        &package_json("pkg1", &[]),
    );
    write_json(
        &dir.path().join("packages/pkg2/package.json"),
        &package_json("pkg2", &[]),
    );

    let config = simple_config(&["packages/*"], &[]);
    let workspace = Workspace::discover(&config, dir.path()).unwrap();

    let changed = vec![dir.path().join("pnpm-lock.yaml")];
    let affected = calculate_affected(
        &workspace,
        &changed,
        &["pnpm-lock.yaml".to_string()],
        dir.path(),
        false,
    );

    assert_eq!(affected.len(), 2);
}

#[test]
fn test_apply_rules_ignores_projects() {
    use nanoom::config::Rule;

    let projects = vec![
        nanoom::workspace::Project {
            name: "pkg1".to_string(),
            path: Path::new("pkg1").to_path_buf(),
            dependencies: vec![],
            dependency_specs: std::collections::HashMap::new(),
            dependents: vec![],
            package_json_version: None,
        },
        nanoom::workspace::Project {
            name: "pkg2".to_string(),
            path: Path::new("pkg2").to_path_buf(),
            dependencies: vec![],
            dependency_specs: std::collections::HashMap::new(),
            dependents: vec![],
            package_json_version: None,
        },
    ];

    let rules = vec![Rule {
        name: "pkg1".to_string(),
        ignore: true,
        shard: vec![],
    }];

    let filtered = apply_rules(projects, &rules, &[]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "pkg2");
}

#[test]
fn test_apply_rules_keeps_non_ignored() {
    use nanoom::config::Rule;

    let projects = vec![nanoom::workspace::Project {
        name: "pkg1".to_string(),
        path: Path::new("pkg1").to_path_buf(),
        dependencies: vec![],
        dependency_specs: std::collections::HashMap::new(),
        dependents: vec![],
        package_json_version: None,
    }];

    let rules = vec![Rule {
        name: "other".to_string(),
        ignore: true,
        shard: vec![],
    }];

    let filtered = apply_rules(projects, &rules, &[]);
    assert_eq!(filtered.len(), 1);
}
