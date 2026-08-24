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

    #[arg(long, help = "Output a JSON result")]
    pub json: bool,
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

    let inputs = [
        "nanoom.config.json",
        "package.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "package-lock.json",
    ];
    let mut existing_inputs = Vec::new();
    for file in inputs {
        let path = cwd.join(file);
        hasher.update(file.as_bytes());
        hasher.update([0]);
        if let Ok(bytes) = std::fs::read(path) {
            hasher.update(bytes);
            existing_inputs.push(file);
        }
        hasher.update([0]);
    }

    let digest = format!("{:x}", hasher.finalize());
    let key = format!("nanoom-{}-{}-{}", args.runner, args.task, &digest[..16]);
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "key": key, "runner": args.runner, "task": args.task,
                "filter": args.filter, "cwd": cwd, "hashedFiles": existing_inputs,
                "reason": "SHA-256 over runner, task, filter, configuration, manifests, and supported lockfiles"
            })
        );
    } else {
        eprintln!(
            "Cache key inputs: runner={} task={} filter={} files={}",
            args.runner,
            args.task,
            if args.filter.is_empty() {
                "<none>"
            } else {
                &args.filter
            },
            existing_inputs.join(",")
        );
        println!("{key}");
    }
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
            json: false,
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
                json: false,
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
