use clap::{Parser, Subcommand};
use nanoom::{
    commands::{
        affected::AffectedArgs, cache_key::CacheKeyArgs, install::InstallArgs, run::RunArgs,
        status::StatusArgs,
    },
    Config, Result,
};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "nanoom", version, about = "Monorepo task runner with affected project detection", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(
        short,
        long,
        global = true,
        help = "Path to nanoom.config.json config file"
    )]
    config: Option<PathBuf>,

    #[arg(short = 'C', long, global = true, help = "Working directory")]
    cwd: Option<PathBuf>,

    #[arg(short, long, global = true, action = clap::ArgAction::Count, help = "Increase verbosity")]
    verbose: u8,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Calculate affected projects and generate matrix")]
    Affected(AffectedArgs),

    #[command(about = "Run tasks on affected projects")]
    Run(RunArgs),

    #[command(about = "Install dependencies")]
    Install(InstallArgs),

    #[command(about = "Aggregate job status")]
    Status(StatusArgs),

    #[command(about = "Generate JSON schema for configuration")]
    Schema {
        #[arg(long, help = "Write schema to file instead of stdout")]
        output: Option<PathBuf>,
    },
    #[command(about = "Generate a deterministic cache key")]
    CacheKey(CacheKeyArgs),
    #[command(about = "Print the nanoom version")]
    Version {
        #[arg(long, help = "Output a JSON result")]
        json: bool,
    },
}

#[tokio::main]
async fn main() {
    let json = std::env::args().any(|arg| arg == "--json");
    if let Err(error) = run().await {
        if json && !matches!(&error, nanoom::Error::ReportedFailure(_)) {
            println!(
                "{}",
                serde_json::json!({"status": "failure", "error": error.to_string()})
            );
        }
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    run_cli(Cli::parse()).await
}

async fn run_cli(cli: Cli) -> Result<()> {
    let config_path = cli
        .config
        .unwrap_or_else(|| PathBuf::from("nanoom.config.json"));
    let cwd = cli
        .cwd
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if cli.verbose > 0 {
        log_invocation(&cwd, &config_path, &cli.command);
    }

    // Schema command does not require a config file; all others do.
    if let Commands::Version { json } = &cli.command {
        if *json {
            println!(
                "{}",
                serde_json::json!({"name": "nanoom", "version": env!("CARGO_PKG_VERSION")})
            );
        } else {
            println!("nanoom {}", env!("CARGO_PKG_VERSION"));
        }
        return Ok(());
    }

    if let Commands::Schema { output, .. } = cli.command {
        if let Some(path) = output {
            nanoom::schema::generate_to_file(&path)?;
            println!("Schema written to {}", path.display());
        } else {
            let schema = nanoom::schema::generate()?;
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
        return Ok(());
    }

    let config = Config::load(&config_path, &cwd)?;

    dispatch(cli.command, &config, &cwd).await
}

async fn dispatch(command: Commands, config: &Config, cwd: &std::path::Path) -> Result<()> {
    match command {
        Commands::Affected(args) => nanoom::commands::affected::execute(args, config, cwd).await,
        Commands::Run(args) => nanoom::commands::run::execute(args, config, cwd).await,
        Commands::Install(args) => nanoom::commands::install::execute(args, config, cwd).await,
        Commands::Status(args) => nanoom::commands::status::execute(args, config).await,
        Commands::Schema { .. } => Ok(()),
        Commands::CacheKey(args) => nanoom::commands::cache_key::execute(args, cwd),
        Commands::Version { .. } => Ok(()),
    }
}

fn log_invocation(cwd: &std::path::Path, config_path: &std::path::Path, command: &Commands) {
    eprintln!(
        "nanoom: cwd={} config={} command={:?}",
        cwd.display(),
        config_path.display(),
        command
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_logging_accepts_each_public_command() {
        let cwd = PathBuf::from(".");
        let config = PathBuf::from("nanoom.config.json");
        log_invocation(&cwd, &config, &Commands::Version { json: false });
        log_invocation(&cwd, &config, &Commands::Schema { output: None });
    }

    #[tokio::test]
    async fn dispatch_handles_early_return_commands() {
        let config: Config =
            serde_json::from_value(serde_json::json!({"group":{"ci":{"tasks":["test"]}}})).unwrap();
        dispatch(
            Commands::Schema { output: None },
            &config,
            PathBuf::from(".").as_path(),
        )
        .await
        .unwrap();
        dispatch(
            Commands::Version { json: false },
            &config,
            PathBuf::from(".").as_path(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_cli_logs_verbose_invocation() {
        run_cli(Cli {
            command: Commands::Version { json: false },
            config: None,
            cwd: None,
            verbose: 1,
        })
        .await
        .unwrap();
    }
}
