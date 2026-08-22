use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Config file not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("Invalid config: {0}")]
    InvalidConfig(String),

    #[error("Config validation error: {0}")]
    ConfigValidation(String),

    #[error("Git error: {0}")]
    Git(#[from] gix::Error),

    #[error("Git error: {0}")]
    GitError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Glob error: {0}")]
    Glob(#[from] globset::Error),

    #[error("Walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Command failed: {command} {args:?} (exit code: {code})")]
    CommandFailed {
        command: String,
        args: Vec<String>,
        code: i32,
    },

    #[error("Task failed: project={project}, task={task}, exit code={code}")]
    TaskFailed {
        project: String,
        task: String,
        code: i32,
    },

    #[error("No common ancestor found between {base} and {head}. Try fetching more history or use --comparison=tip")]
    NoCommonAncestor { base: String, head: String },

    #[error("Shallow repository: need to fetch more history. Run 'git fetch --unshallow' or increase fetch depth")]
    ShallowRepository,

    #[error("Fork repository detected: {0}")]
    ForkRepository(String),

    #[error("Invalid base commit: {0}")]
    InvalidBaseCommit(String),

    #[error("Schema generation error: {0}")]
    SchemaGeneration(String),

    #[error("Package manager not found: {0}")]
    PackageManagerNotFound(String),

    #[error("Invalid runner: {0}. Supported: turbo, yarn, pnpm, nx")]
    InvalidRunner(String),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("Matrix generation error: {0}")]
    MatrixGeneration(String),

    #[error("Status aggregation error: {0}")]
    StatusAggregation(String),
}

pub type Result<T> = std::result::Result<T, Error>;
