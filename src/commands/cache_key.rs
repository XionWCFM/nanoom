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

fn cache_key(args: &CacheKeyArgs, cwd: &Path, config_path: &Path) -> Result<(String, Vec<String>)> {
    if !std::fs::metadata(cwd)?.is_dir() {
        return Err(crate::error::Error::ConfigValidation(
            "cache-key working directory must be a directory".into(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"nanoom-cache-key-v2\0");
    hasher.update(args.runner.as_bytes());
    hasher.update([0]);
    hasher.update(args.task.as_bytes());
    hasher.update([0]);
    hasher.update(args.filter.as_bytes());
    hasher.update([0]);

    let inputs = [
        config_path.to_path_buf(),
        "package.json".into(),
        "pnpm-workspace.yaml".into(),
        ".npmrc".into(),
        ".yarnrc.yml".into(),
        "pnpm-lock.yaml".into(),
        "yarn.lock".into(),
        "package-lock.json".into(),
        "npm-shrinkwrap.json".into(),
    ];
    let mut existing_inputs = Vec::new();
    for file in inputs {
        let path = cwd.join(&file);
        let name = file.to_string_lossy().replace('\\', "/");
        hasher.update(name.as_bytes());
        hasher.update([0]);
        match std::fs::read(path) {
            Ok(bytes) => {
                hasher.update([1]);
                hasher.update((bytes.len() as u64).to_le_bytes());
                hasher.update(bytes);
                existing_inputs.push(name);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => hasher.update([0]),
            Err(error) => return Err(error.into()),
        }
    }

    let digest = format!("{:x}", hasher.finalize());
    let key = format!("nanoom-{}-{}-{}", args.runner, args.task, &digest[..16]);
    Ok((key, existing_inputs))
}

pub fn execute(args: CacheKeyArgs, cwd: &Path, config_path: &Path) -> Result<()> {
    let (key, existing_inputs) = cache_key(&args, cwd, config_path)?;
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
            "◆ nanoom cache-key\n  Inputs: runner={} task={} filter={} files={}",
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
        let key_a = cache_key(&args, dir.path(), Path::new("nanoom.config.json"))
            .unwrap()
            .0;
        let key_b = cache_key(&args, dir.path(), Path::new("nanoom.config.json"))
            .unwrap()
            .0;
        assert_eq!(key_a, key_b);
        execute(args.clone(), dir.path(), Path::new("nanoom.config.json")).unwrap();
        std::fs::write(dir.path().join("nanoom.config.json"), "{\"x\":1}\n").unwrap();
        assert_ne!(
            key_a,
            cache_key(&args, dir.path(), Path::new("nanoom.config.json"))
                .unwrap()
                .0
        );
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
            Path::new("nanoom.config.json"),
        )
        .unwrap();
    }

    #[test]
    fn selected_configuration_optional_input_presence_and_read_errors_are_respected() {
        let dir = tempdir().unwrap();
        let args = CacheKeyArgs {
            runner: "npm".into(),
            task: "test".into(),
            filter: String::new(),
            json: false,
        };
        let selected = Path::new("custom.json");
        std::fs::write(dir.path().join(selected), "{}\n").unwrap();
        let initial = cache_key(&args, dir.path(), selected).unwrap().0;
        std::fs::write(dir.path().join("nanoom.config.json"), "unused").unwrap();
        assert_eq!(cache_key(&args, dir.path(), selected).unwrap().0, initial);
        std::fs::write(dir.path().join(selected), "changed").unwrap();
        assert_ne!(cache_key(&args, dir.path(), selected).unwrap().0, initial);
        let absent = cache_key(&args, dir.path(), selected).unwrap().0;
        std::fs::write(dir.path().join("npm-shrinkwrap.json"), "").unwrap();
        assert_ne!(cache_key(&args, dir.path(), selected).unwrap().0, absent);
        std::fs::create_dir(dir.path().join("yarn.lock")).unwrap();
        assert!(cache_key(&args, dir.path(), selected).is_err());
    }
}
