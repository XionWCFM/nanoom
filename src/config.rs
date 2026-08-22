use crate::error::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,

    #[serde(default)]
    pub group: HashMap<String, GroupConfig>,

    #[serde(rename = "globalDependencies", default)]
    pub global_dependencies: Vec<String>,

    #[serde(default)]
    pub workspace: WorkspaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupConfig {
    pub tasks: Vec<String>,
    pub concurrency: usize,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    #[serde(default)]
    pub ignore: bool,
    #[serde(default)]
    pub isolate: Vec<String>,
    #[serde(default)]
    pub shard: Vec<ShardRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShardRule {
    pub task: String,
    pub shard: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            include: vec!["packages/*".to_string(), "apps/*".to_string()],
            exclude: vec![],
        }
    }
}

impl Config {
    pub fn load(config_path: &Path, cwd: &Path) -> Result<Self> {
        let config_file = cwd.join(config_path);
        if !config_file.exists() {
            return Err(Error::ConfigNotFound(config_path.to_path_buf()));
        }

        let content = std::fs::read_to_string(&config_file)?;
        let config: Config = serde_json::from_str(&content)
            .map_err(|e| Error::InvalidConfig(format!("Failed to parse JSON: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.group.is_empty() {
            return Err(Error::ConfigValidation(
                "At least one group must be defined".to_string(),
            ));
        }

        for (name, group) in &self.group {
            if group.tasks.is_empty() {
                return Err(Error::ConfigValidation(format!(
                    "Group '{}' must have at least one task",
                    name
                )));
            }
            if group.concurrency == 0 {
                return Err(Error::ConfigValidation(format!(
                    "Group '{}' concurrency must be > 0",
                    name
                )));
            }

            for rule in &group.rules {
                if rule.name.is_empty() {
                    return Err(Error::ConfigValidation(format!(
                        "Group '{}' has a rule with empty name",
                        name
                    )));
                }
                for shard in &rule.shard {
                    if shard.shard == 0 {
                        return Err(Error::ConfigValidation(format!(
                            "Group '{}' rule '{}' has shard count 0",
                            name, rule.name
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_group(&self, name: &str) -> Option<&GroupConfig> {
        self.group.get(name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageJson {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub dev_dependencies: HashMap<String, String>,
    #[serde(default)]
    pub peer_dependencies: HashMap<String, String>,
    #[serde(default)]
    pub optional_dependencies: HashMap<String, String>,
    #[serde(default)]
    pub workspaces: WorkspacesField,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WorkspacesField {
    Array(Vec<String>),
    Object(WorkspacesObject),
}

impl Default for WorkspacesField {
    fn default() -> Self {
        WorkspacesField::Array(vec![])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspacesObject {
    pub packages: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PnpmWorkspaceYaml {
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TurboJson {
    #[serde(default)]
    pub pipeline: HashMap<String, TurboPipeline>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TurboPipeline {
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NxJson {
    #[serde(default)]
    pub projects: HashMap<String, String>,
    #[serde(default)]
    pub named_inputs: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub target_defaults: HashMap<String, NxTargetDefaults>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NxTargetDefaults {
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredWorkspace {
    pub name: String,
    pub path: PathBuf,
    pub package_json: PackageJson,
    /// Dependency names (all kinds merged), used for graph traversal.
    pub dependencies: Vec<String>,
    /// Dependency name → declared range (`workspace:*`, `^1.0.0`, ...).
    pub dependency_specs: HashMap<String, String>,
    pub dependents: Vec<String>,
}

impl DiscoveredWorkspace {
    pub fn relative_path(&self, root: &Path) -> PathBuf {
        self.path
            .strip_prefix(root)
            .unwrap_or(&self.path)
            .to_path_buf()
    }
}
