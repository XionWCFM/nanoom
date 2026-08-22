use crate::error::Result;
use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    #[arg(help = "Comma-separated list of job names to check")]
    pub jobs: String,

    #[arg(long, help = "Output format (json, markdown, text)")]
    pub format: Option<String>,
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
        let result = read_job_result(job_name)?;
        has_failure |= result.status == JobStatus::Failure;
        results.push(result);
    }

    let format = args.format.as_deref().unwrap_or("text");
    match format {
        "json" => {
            let output = serde_json::json!({
                "jobs": results,
                "overall": if has_failure { "failure" } else { "success" }
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        "markdown" => {
            println!("# Job Status Summary\n");
            println!("| Job | Status | Duration |");
            println!("|-----|--------|----------|");
            for r in &results {
                let duration = r
                    .duration_ms
                    .map(|d| format!("{}ms", d))
                    .unwrap_or_else(|| "N/A".to_string());
                println!(
                    "| {} | {} | {} |",
                    r.name,
                    status_emoji(&r.status),
                    duration
                );
            }
            println!(
                "\n**Overall: {}**",
                if has_failure {
                    "❌ Failure"
                } else {
                    "✅ Success"
                }
            );
        }
        "text" => {
            for r in &results {
                println!("{}: {}", r.name, status_emoji(&r.status));
            }
            println!(
                "\nOverall: {}",
                if has_failure { "FAILURE" } else { "SUCCESS" }
            );
        }
        _ => {}
    }

    if has_failure {
        std::process::exit(1);
    }

    Ok(())
}

fn read_job_result(job_name: &str) -> Result<JobResult> {
    let output_file =
        std::env::var("GITHUB_OUTPUT").unwrap_or_else(|_| "/tmp/nanoom-output".to_string());
    let content = std::fs::read_to_string(&output_file).unwrap_or_default();

    let status = if content.contains(&format!("{}_result=failure", job_name)) {
        JobStatus::Failure
    } else if content.contains(&format!("{}_result=success", job_name)) {
        JobStatus::Success
    } else if content.contains(&format!("{}_result=cancelled", job_name)) {
        JobStatus::Cancelled
    } else if content.contains(&format!("{}_result=skipped", job_name)) {
        JobStatus::Skipped
    } else {
        JobStatus::Success
    };

    Ok(JobResult {
        name: job_name.to_string(),
        status,
        duration_ms: None,
        url: None,
    })
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
    use serial_test::serial;

    fn temp_output_file(suffix: &str) -> String {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("/tmp/test-output-{}-{}", suffix, n)
    }

    #[test]
    #[serial]
    fn test_read_job_result_failure() {
        let file = temp_output_file("failure");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=failure").unwrap();
        let result = read_job_result("test").unwrap();
        assert_eq!(result.status, JobStatus::Failure);
    }

    #[test]
    #[serial]
    fn test_read_job_result_success() {
        let file = temp_output_file("success");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=success").unwrap();
        let result = read_job_result("test").unwrap();
        assert_eq!(result.status, JobStatus::Success);
    }

    #[test]
    #[serial]
    fn test_read_job_result_cancelled() {
        let file = temp_output_file("cancelled");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=cancelled").unwrap();
        let result = read_job_result("test").unwrap();
        assert_eq!(result.status, JobStatus::Cancelled);
    }

    #[test]
    #[serial]
    fn test_read_job_result_skipped() {
        let file = temp_output_file("skipped");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=skipped").unwrap();
        let result = read_job_result("test").unwrap();
        assert_eq!(result.status, JobStatus::Skipped);
    }

    #[test]
    #[serial]
    fn test_read_job_result_missing_defaults_to_success() {
        let file = temp_output_file("missing");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "other=value").unwrap();
        let result = read_job_result("test").unwrap();
        assert_eq!(result.status, JobStatus::Success);
    }

    #[test]
    #[serial]
    fn test_read_job_result_multiple_jobs() {
        let file = temp_output_file("multi");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "build_result=success\ntest_result=failure").unwrap();
        let build = read_job_result("build").unwrap();
        let test = read_job_result("test").unwrap();
        assert_eq!(build.status, JobStatus::Success);
        assert_eq!(test.status, JobStatus::Failure);
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
