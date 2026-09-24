use crate::error::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct PlanArgs {
    #[command(subcommand)]
    command: PlanCommand,
}

#[derive(Subcommand, Debug, Clone)]
enum PlanCommand {
    #[command(about = "Validate a Plan v1 file and select one assignment")]
    Select {
        #[arg(long, help = "Plan v1 JSON file")]
        input: PathBuf,

        #[arg(long, help = "Plan reference JSON file from the current run")]
        reference: PathBuf,

        #[arg(long, help = "Group name")]
        group: String,

        #[arg(long, help = "Assignment ID")]
        assignment: String,

        #[arg(long, help = "Directory for assignment.json and paths.txt")]
        output_dir: PathBuf,
    },
}

pub fn execute(args: PlanArgs, cwd: &std::path::Path) -> Result<()> {
    match args.command {
        PlanCommand::Select {
            input,
            reference,
            group,
            assignment,
            output_dir,
        } => {
            let input = crate::plan::resolve_path(cwd, &input);
            let reference = crate::plan::resolve_path(cwd, &reference);
            let output_dir = crate::plan::resolve_path(cwd, &output_dir);
            let context = crate::plan::select_assignment(
                &input,
                &reference,
                &group,
                &assignment,
                &output_dir,
            )?;
            println!(
                "{}",
                serde_json::json!({
                    "status": "success",
                    "group": context.group,
                    "assignmentId": context.assignment_id,
                    "itemCount": context.items.len(),
                    "checkoutPathCount": context.checkout_paths.len(),
                    "assignmentFile": output_dir.join("assignment.json"),
                    "pathsFile": output_dir.join("paths.txt"),
                })
            );
            Ok(())
        }
    }
}
