use crate::error::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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

    #[serde(default)]
    pub affected: AffectedConfig,

    #[serde(default)]
    pub checkout: CheckoutConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AffectedConfig {
    #[serde(default = "default_max_fetch_depth")]
    pub max_fetch_depth: usize,
}

impl Default for AffectedConfig {
    fn default() -> Self {
        Self {
            max_fetch_depth: default_max_fetch_depth(),
        }
    }
}

fn default_max_fetch_depth() -> usize {
    2048
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckoutConfig {
    #[serde(default)]
    pub always: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupConfig {
    pub tasks: Vec<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub distribution: Option<DistributionConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing_environment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistributionConfig {
    pub small: DistributionTier,
    pub medium: DistributionTier,
    pub full: DistributionTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DistributionTier {
    pub max_affected_percent: f64,
    pub concurrency: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing_environment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    #[serde(default)]
    pub ignore: bool,
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

        if self.affected.max_fetch_depth == 0 {
            return Err(Error::ConfigValidation(
                "affected.maxFetchDepth must be greater than 0".to_string(),
            ));
        }

        for path in &self.checkout.always {
            let candidate = Path::new(path);
            if path.is_empty()
                || candidate.is_absolute()
                || candidate
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
                || path.contains(['*', '?', '[', ']'])
            {
                return Err(Error::ConfigValidation(format!(
                    "checkout.always entry '{path}' must be a repository-relative directory without glob syntax"
                )));
            }
        }

        for (name, group) in &self.group {
            validate_runner_config(name, "group", &group.runner_labels, &group.timing_environment)?;
            if group.tasks.is_empty() {
                return Err(Error::ConfigValidation(format!(
                    "Group '{}' must have at least one task",
                    name
                )));
            }
            let mut tasks = HashSet::new();
            for task in &group.tasks {
                if task.is_empty() || !tasks.insert(task) {
                    return Err(Error::ConfigValidation(format!(
                        "Group '{name}' has an empty or duplicate task '{task}'"
                    )));
                }
            }
            let mut rules = HashSet::new();
            if let Some(distribution) = &group.distribution {
                let tiers = [
                    ("small", &distribution.small),
                    ("medium", &distribution.medium),
                    ("full", &distribution.full),
                ];
                let mut previous = 0.0;
                for (tier_name, tier) in tiers {
                    validate_runner_config(name, tier_name, &tier.runner_labels, &tier.timing_environment)?;
                    if !tier.max_affected_percent.is_finite()
                        || tier.max_affected_percent <= previous
                        || tier.max_affected_percent > 100.0
                    {
                        return Err(Error::ConfigValidation(format!(
                            "Group '{name}' distribution tier '{tier_name}' must have an increasing maxAffectedPercent in (0, 100]"
                        )));
                    }
                    if tier.concurrency == 0 {
                        return Err(Error::ConfigValidation(format!(
                            "Group '{name}' distribution tier '{tier_name}' must have concurrency greater than 0"
                        )));
                    }
                    previous = tier.max_affected_percent;
                }
                if distribution.full.max_affected_percent != 100.0 {
                    return Err(Error::ConfigValidation(format!(
                        "Group '{name}' distribution tier 'full' must end at maxAffectedPercent 100"
                    )));
                }
            }
            for rule in &group.rules {
                if rule.name.is_empty() {
                    return Err(Error::ConfigValidation(format!(
                        "Group '{}' has a rule with empty name",
                        name
                    )));
                }
                if !rules.insert(&rule.name) {
                    return Err(Error::ConfigValidation(format!(
                        "Group '{name}' has duplicate rule '{}'",
                        rule.name
                    )));
                }
                for shard in &rule.shard {
                    if !tasks.contains(&shard.task) {
                        return Err(Error::ConfigValidation(format!(
                            "Group '{name}' rule '{}' shards unknown task '{}'",
                            rule.name, shard.task
                        )));
                    }
                    if shard.shard == 0 {
                        return Err(Error::ConfigValidation(format!(
                            "Group '{}' rule '{}' has shard count 0",
                            name, rule.name
                        )));
                    }
                }
            }
        }

        for pattern in self
            .global_dependencies
            .iter()
            .chain(&self.workspace.include)
            .chain(&self.workspace.exclude)
        {
            globset::Glob::new(pattern).map_err(|error| {
                Error::ConfigValidation(format!("Invalid glob '{pattern}': {error}"))
            })?;
        }

        Ok(())
    }

    pub fn get_group(&self, name: &str) -> Option<&GroupConfig> {
        self.group.get(name)
    }
}

fn validate_runner_config(group: &str, scope: &str, labels: &Option<Vec<String>>, env: &Option<String>) -> Result<()> {
    if let Some(value) = env {
        if value.trim().is_empty() || value.chars().any(|c| c.is_control()) {
            return Err(Error::ConfigValidation(format!("Group '{group}' {scope} timingEnvironment must not be empty or contain control characters")));
        }
    }
    if let Some(values) = labels {
        if values.is_empty() || values.iter().any(|v| v.trim().is_empty() || v.chars().any(|c| c.is_control())) {
            return Err(Error::ConfigValidation(format!("Group '{group}' {scope} runnerLabels must contain non-empty labels")));
        }
        let mut seen = HashSet::new();
        if values.iter().any(|v| !seen.insert(v)) {
            return Err(Error::ConfigValidation(format!("Group '{group}' {scope} runnerLabels must not contain duplicates")));
        }
    }
    Ok(())
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
