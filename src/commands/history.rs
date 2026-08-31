use crate::error::Result;
use crate::scheduler::{merge_histories, TimingHistory};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct HistoryArgs {
    #[arg(long, required = true, help = "Timing history or sample JSON to merge")]
    pub input: Vec<PathBuf>,

    #[arg(long, required = true, help = "Canonical merged history JSON path")]
    pub output: PathBuf,
}

pub fn execute(args: HistoryArgs) -> Result<()> {
    let histories = args
        .input
        .iter()
        .map(|path| TimingHistory::load(path).map_err(crate::error::Error::InvalidConfig))
        .collect::<Result<Vec<_>>>()?;
    let history = merge_histories(histories);
    std::fs::write(&args.output, serde_json::to_vec_pretty(&history)?)?;
    println!(
        "{}",
        serde_json::json!({
            "status": "success",
            "output": args.output,
            "inputCount": args.input.len(),
            "sampleCount": history.samples.len()
        })
    );
    Ok(())
}
