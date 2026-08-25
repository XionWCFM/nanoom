use nanoom::{config::Config, error::Result};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_load_valid_config() -> Result<()> {
    let dir = tempdir()?;
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{
        "$schema": "nanoom.schema.json",
        "group": {
            "ci": {
                "tasks": ["test", "build"],
                "rules": [
                    { "name": "@org/test", "ignore": true },
                    { "name": "@org/run", "isolate": ["build"] }
                ]
            },
            "e2e": {
                "tasks": ["test:e2e"],
                "rules": [
                    { "name": "@org/core", "shard": [{ "task": "test:e2e", "shard": 4 }] }
                ]
            }
        },
        "globalDependencies": ["yarn.lock", "pnpm-lock.yaml"],
        "workspace": {
            "include": ["packages/*", "apps/*"],
            "exclude": ["packages/deprecated-*"]
        }
    }"#;

    fs::write(&config_path, config_json)?;
    let config = Config::load(&config_path, dir.path())?;

    assert_eq!(config.group.len(), 2);
    assert!(config.group.contains_key("ci"));
    assert!(config.group.contains_key("e2e"));

    let ci_group = &config.group["ci"];
    assert_eq!(ci_group.tasks, vec!["test", "build"]);
    assert_eq!(ci_group.rules.len(), 2);

    let e2e_group = &config.group["e2e"];
    assert_eq!(e2e_group.tasks, vec!["test:e2e"]);
    assert_eq!(e2e_group.rules.len(), 1);

    assert_eq!(
        config.global_dependencies,
        vec!["yarn.lock", "pnpm-lock.yaml"]
    );

    Ok(())
}

#[test]
fn test_load_minimal_config() -> Result<()> {
    let dir = tempdir()?;
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{
        "group": {
            "ci": {
                "tasks": ["test"]
            }
        }
    }"#;

    fs::write(&config_path, config_json)?;
    let config = Config::load(&config_path, dir.path())?;

    assert_eq!(config.group.len(), 1);
    assert_eq!(config.global_dependencies.len(), 0);
    assert_eq!(config.workspace.include, vec!["packages/*", "apps/*"]);
    assert_eq!(config.workspace.exclude.len(), 0);

    Ok(())
}

#[test]
fn test_config_not_found() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nonexistent.json");

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Config file not found"));
}

#[test]
fn test_invalid_json() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nanoom.config.json");

    fs::write(&config_path, "{ invalid json }").unwrap();

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to parse JSON"));
}

#[test]
fn test_unknown_field_rejected() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{
        "group": {
            "ci": {
                "tasks": ["test"],
                "unknown_field": true
            }
        }
    }"#;

    fs::write(&config_path, config_json).unwrap();

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown field"));
}

#[test]
fn test_validate_empty_group() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{ "group": {} }"#;

    fs::write(&config_path, config_json).unwrap();

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("At least one group must be defined"));
}

#[test]
fn test_validate_group_no_tasks() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{
        "group": {
            "ci": {
                "tasks": []
            }
        }
    }"#;

    fs::write(&config_path, config_json).unwrap();

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must have at least one task"));
}

#[test]
fn test_removed_concurrency_is_rejected() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{
        "group": {
            "ci": {
                "tasks": ["test"],
                "concurrency": 0
            }
        }
    }"#;

    fs::write(&config_path, config_json).unwrap();

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown field"));
}

#[test]
fn test_validate_rule_empty_name() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{
        "group": {
            "ci": {
                "tasks": ["test"],
                "rules": [{ "name": "", "ignore": true }]
            }
        }
    }"#;

    fs::write(&config_path, config_json).unwrap();

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty name"));
}

#[test]
fn test_validate_shard_zero() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nanoom.config.json");

    let config_json = r#"{
        "group": {
            "ci": {
                "tasks": ["test"],
                "rules": [{ "name": "pkg", "shard": [{ "task": "test", "shard": 0 }] }]
            }
        }
    }"#;

    fs::write(&config_path, config_json).unwrap();

    let result = Config::load(&config_path, dir.path());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("shard count 0"));
}

#[test]
fn test_validate_rejects_ambiguous_rules_and_invalid_globs() {
    let duplicate_rule: Config = serde_json::from_value(serde_json::json!({
        "group": {"ci": {"tasks": ["test"], "rules": [
            {"name": "app"}, {"name": "app"}
        ]}}
    }))
    .unwrap();
    assert!(duplicate_rule.validate().is_err());

    let conflicting_rule: Config = serde_json::from_value(serde_json::json!({
        "group": {"ci": {"tasks": ["test"], "rules": [{
            "name": "app", "isolate": ["test"],
            "shard": [{"task": "test", "shard": 2}]
        }]}}
    }))
    .unwrap();
    assert!(conflicting_rule.validate().is_err());

    let invalid_glob: Config = serde_json::from_value(serde_json::json!({
        "group": {"ci": {"tasks": ["test"]}},
        "globalDependencies": ["["]
    }))
    .unwrap();
    assert!(invalid_glob.validate().is_err());
}

#[test]
fn test_validate_rejects_duplicate_and_unknown_tasks() {
    for config in [
        serde_json::json!({"group":{"ci":{"tasks":["test","test"]}}}),
        serde_json::json!({"group":{"ci":{"tasks":["test"],"rules":[{"name":"app","isolate":["build"]}]}}}),
        serde_json::json!({"group":{"ci":{"tasks":["test"],"rules":[{"name":"app","shard":[{"task":"build","shard":2}]}]}}}),
    ] {
        let config: Config = serde_json::from_value(config).unwrap();
        assert!(config.validate().is_err());
    }
}

#[test]
fn test_schema_generation() {
    let schema = nanoom::schema::generate().unwrap();
    let schema_str = serde_json::to_string(&schema).unwrap();

    assert!(schema_str.contains("nanoom Configuration"));
    assert!(schema_str.contains("GroupConfig"));
    assert!(schema_str.contains("Rule"));
    assert!(schema_str.contains("ShardRule"));
    assert!(schema_str.contains("globalDependencies"));
    assert!(schema_str.contains("WorkspaceConfig"));
}

#[test]
fn test_workspace_config_defaults() {
    let config = Config {
        schema: None,
        group: std::collections::HashMap::new(),
        global_dependencies: vec![],
        workspace: nanoom::config::WorkspaceConfig::default(),
    };

    assert_eq!(config.workspace.include, vec!["packages/*", "apps/*"]);
    assert_eq!(config.workspace.exclude, Vec::<String>::new());
}
