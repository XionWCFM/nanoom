use crate::error::Result;
use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    #[arg(help = "Comma-separated list of job names to check")]
    pub jobs: String,

    #[arg(long, help = "Output format (json, markdown, text)")]
    pub format: Option<String>,

    #[arg(
        long,
        help = "Comma-separated job=status values, for example build=success,test=failure"
    )]
    pub results: Option<String>,

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
        let result = read_job_result(job_name, args.results.as_deref())?;
        has_failure |= matches!(result.status, JobStatus::Failure | JobStatus::Cancelled);
        results.push(result);
    }

    let format = if args.json {
        "json"
    } else {
        args.format.as_deref().unwrap_or("text")
    };
    match format {
        "json" => {
            let output = serde_json::json!({
                "jobs": results,
                "overall": if has_failure { "failure" } else { "success" },
                "reason": if has_failure { "one or more jobs failed or were cancelled" } else { "no job failed or was cancelled" }
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
        invalid => {
            return Err(crate::error::Error::StatusAggregation(format!(
                "invalid format '{invalid}'; expected text, json, or markdown"
            )))
        }
    }

    if has_failure {
        return Err(crate::error::Error::ReportedFailure(
            "one or more jobs did not succeed".into(),
        ));
    }

    Ok(())
}

fn read_job_result(job_name: &str, explicit_results: Option<&str>) -> Result<JobResult> {
    if let Some(results) = explicit_results {
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
        return Err(crate::error::Error::StatusAggregation(format!(
            "missing result for job '{job_name}'"
        )));
    }
    let output_file = std::env::var("GITHUB_OUTPUT").map_err(|_| {
        crate::error::Error::StatusAggregation(
            "status requires --results or a GITHUB_OUTPUT file".into(),
        )
    })?;
    let content = std::fs::read_to_string(&output_file).map_err(|error| {
        crate::error::Error::StatusAggregation(format!(
            "cannot read GITHUB_OUTPUT '{output_file}': {error}"
        ))
    })?;

    let status = if content.contains(&format!("{}_result=failure", job_name)) {
        JobStatus::Failure
    } else if content.contains(&format!("{}_result=success", job_name)) {
        JobStatus::Success
    } else if content.contains(&format!("{}_result=cancelled", job_name)) {
        JobStatus::Cancelled
    } else if content.contains(&format!("{}_result=skipped", job_name)) {
        JobStatus::Skipped
    } else {
        return Err(crate::error::Error::StatusAggregation(format!(
            "missing result for job '{job_name}'"
        )));
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
        std::env::temp_dir()
            .join(format!("nanoom-test-output-{}-{}", suffix, n))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    #[serial]
    fn test_read_job_result_failure() {
        let file = temp_output_file("failure");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=failure").unwrap();
        let result = read_job_result("test", None).unwrap();
        assert_eq!(result.status, JobStatus::Failure);
    }

    #[test]
    #[serial]
    fn test_read_job_result_success() {
        let file = temp_output_file("success");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=success").unwrap();
        let result = read_job_result("test", None).unwrap();
        assert_eq!(result.status, JobStatus::Success);
    }

    #[test]
    #[serial]
    fn test_read_job_result_cancelled() {
        let file = temp_output_file("cancelled");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=cancelled").unwrap();
        let result = read_job_result("test", None).unwrap();
        assert_eq!(result.status, JobStatus::Cancelled);
    }

    #[test]
    #[serial]
    fn test_read_job_result_skipped() {
        let file = temp_output_file("skipped");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "test_result=skipped").unwrap();
        let result = read_job_result("test", None).unwrap();
        assert_eq!(result.status, JobStatus::Skipped);
    }

    #[test]
    #[serial]
    fn test_read_job_result_missing_is_an_error() {
        let file = temp_output_file("missing");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "other=value").unwrap();
        assert!(read_job_result("test", None).is_err());
    }

    #[test]
    #[serial]
    fn invalid_explicit_status_and_missing_output_are_errors() {
        let invalid = read_job_result("test", Some("test=unknown")).unwrap_err();
        assert!(
            matches!(invalid, crate::error::Error::StatusAggregation(message) if message.contains("invalid status"))
        );
        std::env::remove_var("GITHUB_OUTPUT");
        let missing = read_job_result("test", None).unwrap_err();
        assert!(
            matches!(missing, crate::error::Error::StatusAggregation(message) if message.contains("requires"))
        );
    }

    #[test]
    #[serial]
    fn unreadable_output_is_an_error() {
        std::env::set_var("GITHUB_OUTPUT", "/path/that/does/not/exist");
        let error = read_job_result("test", None).unwrap_err();
        assert!(
            matches!(error, crate::error::Error::StatusAggregation(message) if message.contains("cannot read"))
        );
    }

    #[test]
    #[serial]
    fn test_read_job_result_multiple_jobs() {
        let file = temp_output_file("multi");
        std::env::set_var("GITHUB_OUTPUT", &file);
        std::fs::write(&file, "build_result=success\ntest_result=failure").unwrap();
        let build = read_job_result("build", None).unwrap();
        let test = read_job_result("test", None).unwrap();
        assert_eq!(build.status, JobStatus::Success);
        assert_eq!(test.status, JobStatus::Failure);
    }

    #[test]
    fn explicit_results_take_precedence_over_github_output() {
        let result = read_job_result("matrix", Some("matrix=failure,other=success")).unwrap();
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

    #[tokio::test]
    async fn execute_reports_success_and_rejects_invalid_format() {
        let config: crate::Config = serde_json::from_value(serde_json::json!({
            "group": {"ci": {"tasks": ["test"]}}
        }))
        .unwrap();
        execute(
            StatusArgs {
                jobs: "build".into(),
                format: Some("json".into()),
                results: Some("build=success".into()),
                json: false,
            },
            &config,
        )
        .await
        .unwrap();
        let error = execute(
            StatusArgs {
                jobs: "build".into(),
                format: Some("xml".into()),
                results: Some("build=success".into()),
                json: false,
            },
            &config,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, crate::error::Error::StatusAggregation(message) if message.contains("invalid format"))
        );
    }

    #[tokio::test]
    async fn execute_covers_text_markdown_and_reported_failure() {
        let config: crate::Config = serde_json::from_value(serde_json::json!({
            "group": {"ci": {"tasks": ["test"]}}
        }))
        .unwrap();
        for format in ["text", "markdown"] {
            execute(
                StatusArgs {
                    jobs: "build".into(),
                    format: Some(format.into()),
                    results: Some("build=success".into()),
                    json: false,
                },
                &config,
            )
            .await
            .unwrap();
        }
        let error = execute(
            StatusArgs {
                jobs: "build".into(),
                format: Some("text".into()),
                results: Some("build=failure".into()),
                json: false,
            },
            &config,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, crate::error::Error::ReportedFailure(_)));
    }
}
