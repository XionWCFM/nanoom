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
    CacheKey(CacheKeyArgs),
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config_path = cli
        .config
        .unwrap_or_else(|| PathBuf::from("nanoom.config.json"));
    let cwd = cli
        .cwd
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Schema command does not require a config file; all others do.
    if let Commands::Version = cli.command {
        println!("nanoom {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Commands::Schema { output } = cli.command {
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

    match cli.command {
        Commands::Affected(args) => nanoom::commands::affected::execute(args, &config, &cwd).await,
        Commands::Run(args) => nanoom::commands::run::execute(args, &config, &cwd).await,
        Commands::Install(args) => nanoom::commands::install::execute(args, &config, &cwd).await,
        Commands::Status(args) => nanoom::commands::status::execute(args, &config).await,
        Commands::Schema { .. } => Ok(()),
        Commands::CacheKey(args) => nanoom::commands::cache_key::execute(args, &cwd),
        Commands::Version => Ok(()),
    }
}
