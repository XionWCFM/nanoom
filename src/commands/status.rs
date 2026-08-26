use crate::error::Result;
use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    #[arg(help = "Comma-separated list of job names to check")]
    pub jobs: String,

    #[arg(
        long,
        required = true,
        help = "Comma-separated job=status values, for example build=success,test=failure"
    )]
    pub results: String,

    #[arg(long, help = "Output a JSON result")]
    pub json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub name: String,
    pub status: JobStatus,
    pub duration_ms: Option<u64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Success,
    Failure,
    Cancelled,
    Skipped,
}

pub async fn execute(args: StatusArgs, _config: &crate::Config) -> Result<()> {
    let job_names: Vec<String> = args.jobs.split(',').map(|s| s.trim().to_string()).collect();

    let mut results = Vec::new();
    let mut has_failure = false;

    for job_name in &job_names {
        let result = read_job_result(job_name, &args.results)?;
        has_failure |= matches!(result.status, JobStatus::Failure | JobStatus::Cancelled);
        results.push(result);
    }

    if args.json {
        let output = serde_json::json!({
            "jobs": results,
            "overall": if has_failure { "failure" } else { "success" },
            "reason": if has_failure { "one or more jobs failed or were cancelled" } else { "no job failed or was cancelled" }
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("◆ nanoom status");
        println!("  Observed jobs:");
        for r in &results {
            println!("  - {}: {}", r.name, status_emoji(&r.status));
        }
        println!(
            "  Result: {}",
            if has_failure { "FAILURE" } else { "SUCCESS" }
        );
        println!(
            "  Why: {}",
            if has_failure {
                "one or more jobs failed or were cancelled"
            } else {
                "no job failed or was cancelled"
            }
        );
    }

    if has_failure {
        return Err(crate::error::Error::ReportedFailure(
            "one or more jobs did not succeed".into(),
        ));
    }

    Ok(())
}

fn read_job_result(job_name: &str, results: &str) -> Result<JobResult> {
    if let Some((_, status)) = results
        .split(',')
        .map(str::trim)
        .filter_map(|entry| entry.split_once('='))
        .find(|(name, _)| name.trim() == job_name)
    {
        let status = match status.trim() {
            "success" => JobStatus::Success,
            "failure" => JobStatus::Failure,
            "cancelled" => JobStatus::Cancelled,
            "skipped" => JobStatus::Skipped,
            invalid => {
                return Err(crate::error::Error::StatusAggregation(format!(
                    "invalid status '{invalid}' for job '{job_name}'"
                )))
            }
        };
        return Ok(JobResult {
            name: job_name.to_string(),
            status,
            duration_ms: None,
            url: None,
        });
    }
    Err(crate::error::Error::StatusAggregation(format!(
        "missing result for job '{job_name}'"
    )))
}

fn status_emoji(status: &JobStatus) -> &'static str {
    match status {
        JobStatus::Success => "✅",
        JobStatus::Failure => "❌",
        JobStatus::Cancelled => "⏭️",
        JobStatus::Skipped => "⏭️",
    }
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_job_result_failure() {
        let result = read_job_result("test", "test=failure").unwrap();
        assert_eq!(result.status, JobStatus::Failure);
    }

    #[test]
    fn test_read_job_result_success() {
        let result = read_job_result("test", "test=success").unwrap();
        assert_eq!(result.status, JobStatus::Success);
    }

    #[test]
    fn test_read_job_result_cancelled() {
        let result = read_job_result("test", "test=cancelled").unwrap();
        assert_eq!(result.status, JobStatus::Cancelled);
    }

    #[test]
    fn test_read_job_result_skipped() {
        let result = read_job_result("test", "test=skipped").unwrap();
        assert_eq!(result.status, JobStatus::Skipped);
    }

    #[test]
    fn test_read_job_result_missing_is_an_error() {
        assert!(read_job_result("test", "other=success").is_err());
    }

    #[test]
    fn test_read_job_result_multiple_jobs() {
        let build = read_job_result("build", "build=success,test=failure").unwrap();
        let test = read_job_result("test", "build=success,test=failure").unwrap();
        assert_eq!(build.status, JobStatus::Success);
        assert_eq!(test.status, JobStatus::Failure);
    }

    #[test]
    fn explicit_results_are_the_only_status_source() {
        let result = read_job_result("matrix", "matrix=failure,other=success").unwrap();
        assert_eq!(result.status, JobStatus::Failure);
    }

    #[test]
    fn test_status_emoji() {
        assert_eq!(status_emoji(&JobStatus::Success), "✅");
        assert_eq!(status_emoji(&JobStatus::Failure), "❌");
        assert_eq!(status_emoji(&JobStatus::Cancelled), "⏭️");
        assert_eq!(status_emoji(&JobStatus::Skipped), "⏭️");
    }

    #[test]
    fn test_job_result_serialization() {
        let jr = JobResult {
            name: "test".into(),
            status: JobStatus::Success,
            duration_ms: Some(1000),
            url: Some("http://example.com".into()),
        };
        let json = serde_json::to_string(&jr).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("success"));
        assert!(json.contains("1000"));
    }
}
