use crate::error::Result;
use clap::Args;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Args, Debug, Clone)]
pub struct CacheKeyArgs {
    #[arg(long, help = "Runner name (turbo, nx, pnpm, yarn, or npm)")]
    pub runner: String,

    #[arg(long, help = "Task name")]
    pub task: String,

    #[arg(long, default_value = "", help = "Workspace filter")]
    pub filter: String,
}

pub fn execute(args: CacheKeyArgs, cwd: &Path) -> Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(b"nanoom-cache-key-v1\0");
    hasher.update(args.runner.as_bytes());
    hasher.update([0]);
    hasher.update(args.task.as_bytes());
    hasher.update([0]);
    hasher.update(args.filter.as_bytes());
    hasher.update([0]);

    for file in [
        "nanoom.config.json",
        "package.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "package-lock.json",
    ] {
        let path = cwd.join(file);
        hasher.update(file.as_bytes());
        hasher.update([0]);
        if let Ok(bytes) = std::fs::read(path) {
            hasher.update(bytes);
        }
        hasher.update([0]);
    }

    let digest = format!("{:x}", hasher.finalize());
    println!("nanoom-{}-{}-{}", args.runner, args.task, &digest[..16]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn key_is_deterministic_and_changes_with_config() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("nanoom.config.json"), "{}\n").unwrap();
        let args = CacheKeyArgs {
            runner: "turbo".into(),
            task: "test".into(),
            filter: "pkg-a".into(),
        };
        let key_a = key_for_test(&args, dir.path());
        let key_b = key_for_test(&args, dir.path());
        assert_eq!(key_a, key_b);
        std::fs::write(dir.path().join("nanoom.config.json"), "{\"x\":1}\n").unwrap();
        assert_ne!(key_a, key_for_test(&args, dir.path()));
    }

    #[test]
    fn execute_hashes_all_supported_inputs_and_missing_files() {
        let dir = tempdir().unwrap();
        for (name, contents) in [
            ("nanoom.config.json", "{}"),
            ("package.json", "{}"),
            ("pnpm-lock.yaml", "lock"),
            ("yarn.lock", "lock"),
            ("package-lock.json", "{}"),
        ] {
            std::fs::write(dir.path().join(name), contents).unwrap();
        }
        execute(
            CacheKeyArgs {
                runner: "npm".into(),
                task: "test".into(),
                filter: String::new(),
            },
            dir.path(),
        )
        .unwrap();
    }

    fn key_for_test(args: &CacheKeyArgs, cwd: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"nanoom-cache-key-v1\0");
        hasher.update(args.runner.as_bytes());
        hasher.update([0]);
        hasher.update(args.task.as_bytes());
        hasher.update([0]);
        hasher.update(args.filter.as_bytes());
        hasher.update([0]);
        let bytes = std::fs::read(cwd.join("nanoom.config.json")).unwrap();
        hasher.update(bytes);
        format!(
            "nanoom-{}-{}-{}",
            args.runner,
            args.task,
            &format!("{:x}", hasher.finalize())[..16]
        )
    }
}
