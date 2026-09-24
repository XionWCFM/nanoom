use crate::affected::AffectedOutput;
use crate::error::{Error, Result};
use crate::scheduler::PredictionSources;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

pub const PLAN_VERSION: u32 = 1;
const MAX_MATRIX_ROWS: usize = 256;
const MAX_OUTPUT_UTF16_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanContext {
    pub repository: String,
    pub workflow: String,
    pub run_id: String,
    pub producer_attempt: u64,
    pub planning_job: String,
    pub base: String,
    pub head: String,
    pub task_runner: String,
    #[serde(default)]
    pub prediction_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction_artifact: Option<ArtifactDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_artifact: Option<ArtifactDigest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactDigest {
    pub name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanProvenance {
    pub repository: String,
    pub workflow: String,
    pub run_id: String,
    pub producer_attempt: u64,
    pub planning_job: String,
    pub base: String,
    pub head: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurrentRun {
    pub repository: String,
    pub workflow: String,
    pub run_id: String,
    pub attempt: u64,
    pub base: String,
    pub head: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanReference {
    pub version: u32,
    pub artifact_name: String,
    pub sha256: String,
    pub provenance: PlanProvenance,
    pub current: CurrentRun,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub provenance: PlanProvenance,
    pub task_runner: String,
    pub prediction_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction_artifact: Option<ArtifactDigest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_artifact: Option<ArtifactDigest>,
    pub has_change: bool,
    pub groups: BTreeMap<String, PlanGroup>,
    pub assignment_count: usize,
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanGroup {
    pub assignments: Vec<PlanAssignment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanAssignment {
    pub assignment_id: String,
    pub items: Vec<PlanItem>,
    pub checkout_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_preparation_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction_sources: Option<PredictionSources>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation_prediction_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation_sample_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling_mode: Option<String>,
    pub prediction_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanItem {
    pub group: String,
    pub name: String,
    pub path: String,
    pub task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_shards: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentContext {
    pub version: u32,
    pub plan_sha256: String,
    pub provenance: PlanProvenance,
    pub current: CurrentRun,
    pub task_runner: String,
    pub group: String,
    pub assignment_id: String,
    pub items: Vec<PlanItem>,
    pub checkout_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_preparation_ms: Option<u64>,
    pub prediction_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation_prediction_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation_sample_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_environment: Option<String>,
}

impl PlanContext {
    fn provenance(&self) -> PlanProvenance {
        PlanProvenance {
            repository: self.repository.clone(),
            workflow: self.workflow.clone(),
            run_id: self.run_id.clone(),
            producer_attempt: self.producer_attempt,
            planning_job: self.planning_job.clone(),
            base: self.base.clone(),
            head: self.head.clone(),
        }
    }

    fn validate(&self) -> Result<()> {
        validate_provenance(&self.provenance())?;
        if self.task_runner.trim().is_empty() {
            return Err(invalid("plan context taskRunner must not be empty"));
        }
        for artifact in [&self.prediction_artifact, &self.model_artifact]
            .into_iter()
            .flatten()
        {
            validate_artifact_digest(artifact)?;
        }
        Ok(())
    }
}

impl Plan {
    pub fn from_affected(
        output: &AffectedOutput,
        matrix: &Value,
        context: &PlanContext,
        expected_task_runner: &str,
        cwd: &Path,
    ) -> Result<Self> {
        context.validate()?;
        if context.task_runner != expected_task_runner {
            return Err(invalid(format!(
                "plan context taskRunner '{}' does not match resolved affected runner '{expected_task_runner}'",
                context.task_runner
            )));
        }
        let comparison = output
            .diagnostics
            .as_ref()
            .map(|diagnostics| &diagnostics.comparison)
            .ok_or_else(|| invalid("affected result has no resolved base/head diagnostics"))?;
        if !same_sha(&context.base, &comparison.base_commit)
            || !same_sha(&context.head, &comparison.head_commit)
        {
            return Err(invalid(
                "plan context base/head do not match the resolved affected commits",
            ));
        }

        let mut groups = BTreeMap::new();
        let repo_root = cwd.canonicalize()?;
        for group_name in output.group.keys() {
            let rows = matrix
                .get(group_name)
                .and_then(|group| group.get("include"))
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    invalid(format!("matrix group '{group_name}' has no include array"))
                })?;
            let mut assignments = Vec::with_capacity(rows.len());
            for (index, row) in rows.iter().enumerate() {
                assignments.push(assignment_from_matrix(
                    group_name, index, row, cwd, &repo_root,
                )?);
            }
            groups.insert(group_name.clone(), PlanGroup { assignments });
        }

        let assignment_count = groups.values().map(|group| group.assignments.len()).sum();
        let item_count = groups
            .values()
            .flat_map(|group| &group.assignments)
            .map(|assignment| assignment.items.len())
            .sum();
        let plan = Self {
            version: PLAN_VERSION,
            provenance: context.provenance(),
            task_runner: context.task_runner.clone(),
            prediction_reason: context.prediction_reason.clone(),
            prediction_artifact: context.prediction_artifact.clone(),
            model_artifact: context.model_artifact.clone(),
            has_change: item_count > 0,
            groups,
            assignment_count,
            item_count,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != PLAN_VERSION {
            return Err(invalid(format!(
                "unsupported Plan version {}; expected {PLAN_VERSION}",
                self.version
            )));
        }
        validate_provenance(&self.provenance)?;
        if self.task_runner.trim().is_empty() {
            return Err(invalid("Plan taskRunner must not be empty"));
        }
        for artifact in [&self.prediction_artifact, &self.model_artifact]
            .into_iter()
            .flatten()
        {
            validate_artifact_digest(artifact)?;
        }

        let mut assignments = 0;
        let mut items = 0;
        let mut identities = HashSet::new();
        for (group_name, group) in &self.groups {
            if group_name.trim().is_empty() {
                return Err(invalid("Plan group names must not be empty"));
            }
            let mut assignment_ids = HashSet::new();
            for assignment in &group.assignments {
                assignments += 1;
                if assignment.assignment_id.trim().is_empty()
                    || !assignment_ids.insert(assignment.assignment_id.as_str())
                {
                    return Err(invalid(format!(
                        "Plan group '{group_name}' has an empty or duplicate assignmentId"
                    )));
                }
                if assignment.items.is_empty() {
                    return Err(invalid(format!(
                        "Plan assignment '{group_name}/{}' has no items",
                        assignment.assignment_id
                    )));
                }
                validate_assignment_prediction(
                    assignment.predicted_preparation_ms,
                    assignment.preparation_prediction_source.as_deref(),
                    assignment.preparation_sample_count,
                    assignment.scheduling_mode.as_deref(),
                )?;
                validate_sorted_paths(group_name, &assignment.checkout_paths)?;
                for item in &assignment.items {
                    items += 1;
                    if item.group != *group_name
                        || item.name.trim().is_empty()
                        || item.task.trim().is_empty()
                        || !valid_relative_path(&item.path)
                    {
                        return Err(invalid(format!(
                            "Plan item '{}/{}/{}' has invalid identity or path {:?} (valid={}, expected group='{}')",
                            item.group, item.name, item.task, item.path, valid_relative_path(&item.path), group_name
                        )));
                    }
                    if !assignment.checkout_paths.contains(&item.path) {
                        return Err(invalid(format!(
                            "Plan item path '{}' is not in checkout paths for '{group_name}/{}': {:?}",
                            item.path, assignment.assignment_id, assignment.checkout_paths
                        )));
                    }
                    if !identities.insert((
                        item.group.as_str(),
                        item.name.as_str(),
                        item.task.as_str(),
                        item.shard,
                        item.total_shards,
                    )) {
                        return Err(invalid(format!(
                            "Plan item '{}/{}/{}' is assigned more than once",
                            item.group, item.name, item.task
                        )));
                    }
                    match (item.shard, item.total_shards) {
                        (None, None) => {}
                        (Some(shard), Some(total)) if shard > 0 && shard <= total => {}
                        _ => return Err(invalid("Plan item has an invalid shard layout")),
                    }
                }
            }
        }
        if assignments != self.assignment_count || items != self.item_count {
            return Err(invalid(
                "Plan assignment/item counts do not match its contents",
            ));
        }
        if self.has_change != (items > 0) {
            return Err(invalid("Plan hasChange does not match its item count"));
        }
        Ok(())
    }
}

pub fn write_affected_plan(
    output: &AffectedOutput,
    matrix: &Value,
    expected_task_runner: &str,
    cwd: &Path,
    context_path: &Path,
    plan_path: &Path,
) -> Result<String> {
    let context_bytes = std::fs::read(context_path)?;
    let context: PlanContext = serde_json::from_slice(&context_bytes)?;
    let plan = Plan::from_affected(output, matrix, &context, expected_task_runner, cwd)?;
    let plan_bytes = serde_json::to_vec_pretty(&plan)?;
    let reference = PlanReference::for_plan(&plan, &plan_bytes)?;
    let compact = compact_output(&plan, &reference)?;
    if let Some(parent) = plan_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(plan_path, plan_bytes)?;
    Ok(compact)
}

impl PlanReference {
    fn for_plan(plan: &Plan, plan_bytes: &[u8]) -> Result<Self> {
        let provenance = plan.provenance.clone();
        Ok(Self {
            version: PLAN_VERSION,
            artifact_name: artifact_name(&provenance)?,
            sha256: sha256(plan_bytes),
            current: CurrentRun {
                repository: provenance.repository.clone(),
                workflow: provenance.workflow.clone(),
                run_id: provenance.run_id.clone(),
                attempt: provenance.producer_attempt,
                base: provenance.base.clone(),
                head: provenance.head.clone(),
            },
            provenance,
        })
    }

    pub fn validate(&self, plan: &Plan, plan_bytes: &[u8]) -> Result<()> {
        if self.version != PLAN_VERSION {
            return Err(invalid(format!(
                "unsupported Plan reference version {}; expected {PLAN_VERSION}",
                self.version
            )));
        }
        validate_provenance(&self.provenance)?;
        if self.sha256 != sha256(plan_bytes) {
            return Err(invalid("Plan SHA-256 does not match its reference"));
        }
        if self.artifact_name != artifact_name(&self.provenance)? {
            return Err(invalid("Plan artifact name does not match its provenance"));
        }
        if self.current.repository != self.provenance.repository
            || self.current.workflow != self.provenance.workflow
            || self.current.run_id != self.provenance.run_id
            || !same_sha(&self.current.base, &self.provenance.base)
            || !same_sha(&self.current.head, &self.provenance.head)
        {
            return Err(invalid(
                "Plan reference belongs to a different repository, workflow, run, or head",
            ));
        }
        if self.current.attempt < self.provenance.producer_attempt {
            return Err(invalid(
                "Plan producerAttempt is newer than the current run attempt",
            ));
        }
        if plan.provenance != self.provenance {
            return Err(invalid("Plan provenance does not match its reference"));
        }
        plan.validate()
    }
}

pub fn select_assignment(
    plan_path: &Path,
    reference_path: &Path,
    group_name: &str,
    assignment_id: &str,
    output_dir: &Path,
) -> Result<AssignmentContext> {
    let plan_bytes = std::fs::read(plan_path)?;
    let plan: Plan = serde_json::from_slice(&plan_bytes)?;
    let reference: PlanReference = serde_json::from_slice(&std::fs::read(reference_path)?)?;
    reference.validate(&plan, &plan_bytes)?;
    let assignment = plan
        .groups
        .get(group_name)
        .and_then(|group| {
            group
                .assignments
                .iter()
                .find(|assignment| assignment.assignment_id == assignment_id)
        })
        .ok_or_else(|| {
            invalid(format!(
                "Plan assignment '{group_name}/{assignment_id}' does not exist"
            ))
        })?;
    let context = AssignmentContext {
        version: PLAN_VERSION,
        plan_sha256: reference.sha256.clone(),
        provenance: plan.provenance.clone(),
        current: reference.current.clone(),
        task_runner: plan.task_runner.clone(),
        group: group_name.to_owned(),
        assignment_id: assignment_id.to_owned(),
        items: assignment.items.clone(),
        checkout_paths: assignment.checkout_paths.clone(),
        predicted_duration_ms: assignment.predicted_duration_ms,
        predicted_preparation_ms: assignment.predicted_preparation_ms,
        prediction_reason: assignment.prediction_reason.clone(),
        preparation_prediction_source: assignment.preparation_prediction_source.clone(),
        preparation_sample_count: assignment.preparation_sample_count,
        scheduling_mode: assignment.scheduling_mode.clone(),
        runner_labels: assignment.runner_labels.clone(),
        timing_environment: assignment.timing_environment.clone(),
    };
    std::fs::create_dir_all(output_dir)?;
    std::fs::write(
        output_dir.join("assignment.json"),
        serde_json::to_vec_pretty(&context)?,
    )?;
    std::fs::write(
        output_dir.join("paths.txt"),
        format!("{}\n", assignment.checkout_paths.join("\n")),
    )?;
    Ok(context)
}

pub fn compact_output(plan: &Plan, reference: &PlanReference) -> Result<String> {
    let mut groups = serde_json::Map::new();
    for (group_name, group) in &plan.groups {
        if group.assignments.len() > MAX_MATRIX_ROWS {
            return Err(invalid(format!(
                "group '{group_name}' compact matrix has {} rows; maximum is {MAX_MATRIX_ROWS}",
                group.assignments.len()
            )));
        }
        let include: Vec<Value> = group
            .assignments
            .iter()
            .map(|assignment| {
                let mut row = serde_json::Map::new();
                row.insert("group".into(), Value::String(group_name.clone()));
                row.insert(
                    "assignmentId".into(),
                    Value::String(assignment.assignment_id.clone()),
                );
                if let Some(labels) = &assignment.runner_labels {
                    row.insert("runnerLabels".into(), serde_json::json!(labels));
                }
                if let Some(environment) = &assignment.timing_environment {
                    row.insert(
                        "timingEnvironment".into(),
                        Value::String(environment.clone()),
                    );
                }
                if let Some(duration) = assignment.predicted_duration_ms {
                    row.insert("predictedDurationMs".into(), serde_json::json!(duration));
                }
                if let Some(duration) = assignment.predicted_preparation_ms {
                    row.insert("predictedPreparationMs".into(), serde_json::json!(duration));
                }
                if let Some(reason) = &assignment.prediction_sources {
                    row.insert("predictionSources".into(), serde_json::json!(reason));
                }
                if let Some(source) = &assignment.preparation_prediction_source {
                    row.insert(
                        "preparationPredictionSource".into(),
                        serde_json::json!(source),
                    );
                }
                if let Some(count) = assignment.preparation_sample_count {
                    row.insert("preparationSampleCount".into(), serde_json::json!(count));
                }
                if let Some(mode) = &assignment.scheduling_mode {
                    row.insert("schedulingMode".into(), serde_json::json!(mode));
                    row.insert(
                        "predictionReason".into(),
                        Value::String(assignment.prediction_reason.clone()),
                    );
                }
                Value::Object(row)
            })
            .collect();
        groups.insert(group_name.clone(), serde_json::json!({"include": include}));
    }
    let value = serde_json::json!({
        "has_change": plan.has_change,
        "plan": reference,
        "groups": groups,
        "result": {
            "status": "success",
            "hasChange": plan.has_change,
            "groupCount": plan.groups.len(),
            "assignmentCount": plan.assignment_count,
            "itemCount": plan.item_count,
            "reason": plan.prediction_reason,
        }
    });
    let output = serde_json::to_string(&value)?;
    let bytes = output.encode_utf16().count() * 2 + 2;
    if bytes > MAX_OUTPUT_UTF16_BYTES {
        let groups = plan
            .groups
            .keys()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(invalid(format!(
            "compact Plan outputs exceed the 1 MiB UTF-16 limit: {bytes} bytes; groups [{}]; reason: bounded output size exceeded",
            groups
        )));
    }
    Ok(output)
}

pub fn resolve_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn assignment_from_matrix(
    group: &str,
    index: usize,
    row: &Value,
    cwd: &Path,
    repo_root: &Path,
) -> Result<PlanAssignment> {
    let (assignment_id, item_rows): (String, Vec<&Value>) = if let Some(rows) =
        row.get("items").and_then(Value::as_array)
    {
        let id = row
            .get("assignmentId")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(format!("matrix assignment in group '{group}' has no id")))?;
        (id.to_owned(), rows.iter().collect())
    } else {
        (format!("{group}-{:04}", index + 1), vec![row])
    };
    let items = item_rows
        .into_iter()
        .map(|item| {
            Ok(PlanItem {
                group: item
                    .get("group")
                    .and_then(Value::as_str)
                    .unwrap_or(group)
                    .to_owned(),
                name: required_string(item, "name", group)?,
                path: workspace_relative_path(
                    &required_string(item, "path", group)?,
                    cwd,
                    repo_root,
                )?,
                task: required_string(item, "task", group)?,
                shard: optional_usize(item, "shard")?,
                total_shards: optional_usize(item, "totalShards")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let checkout = row
        .get("checkout")
        .and_then(|value| value.get("sparseCheckout"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            invalid(format!(
                "matrix assignment in group '{group}' has no checkout"
            ))
        })?;
    let checkout_paths = if checkout.is_empty() {
        Vec::new()
    } else {
        checkout
            .split('\n')
            .map(|path| workspace_relative_path(path, cwd, repo_root))
            .collect::<Result<Vec<_>>>()?
    };
    let prediction_sources = row
        .get("predictionSources")
        .cloned()
        .map(serde_json::from_value::<PredictionSources>)
        .transpose()?;
    Ok(PlanAssignment {
        assignment_id,
        items,
        checkout_paths,
        predicted_duration_ms: row.get("predictedDurationMs").and_then(Value::as_u64),
        predicted_preparation_ms: row.get("predictedPreparationMs").and_then(Value::as_u64),
        prediction_sources,
        preparation_prediction_source: row
            .get("preparationPredictionSource")
            .and_then(Value::as_str)
            .map(str::to_owned),
        preparation_sample_count: row
            .get("preparationSampleCount")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok()),
        scheduling_mode: row
            .get("schedulingMode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        prediction_reason: row
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("static assignment without prediction")
            .to_owned(),
        runner_labels: row
            .get("runnerLabels")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        timing_environment: row
            .get("timingEnvironment")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn required_string(value: &Value, field: &str, group: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("matrix group '{group}' item has no {field}")))
}

fn workspace_relative_path(path: &str, cwd: &Path, repo_root: &Path) -> Result<String> {
    let path = Path::new(path);
    let relative = if path.is_absolute() {
        path.strip_prefix(cwd)
            .or_else(|_| path.strip_prefix(repo_root))
            .map_err(|_| {
                invalid(format!(
                    "Plan item path '{}' is outside the affected repository",
                    path.display()
                ))
            })?
    } else {
        path
    };
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(value) => {
                components.push(value.to_string_lossy().into_owned())
            }
            std::path::Component::CurDir => {}
            _ => {
                return Err(invalid(format!(
                    "Plan item path '{}' is not a safe relative path",
                    relative.display()
                )))
            }
        }
    }
    let normalized = if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    };
    if !valid_relative_path(&normalized) {
        return Err(invalid(format!(
            "Plan item path '{}' is not a safe relative path",
            normalized
        )));
    }
    Ok(normalized)
}

fn optional_usize(value: &Value, field: &str) -> Result<Option<usize>> {
    value
        .get(field)
        .map(|number| {
            number
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid(format!("matrix item has invalid {field}")))
        })
        .transpose()
}

fn validate_provenance(provenance: &PlanProvenance) -> Result<()> {
    for (field, value) in [
        ("repository", provenance.repository.as_str()),
        ("workflow", provenance.workflow.as_str()),
        ("runId", provenance.run_id.as_str()),
        ("planningJob", provenance.planning_job.as_str()),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(invalid(format!(
                "Plan {field} must be non-empty and have no controls"
            )));
        }
    }
    if !provenance.run_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid("Plan runId must contain decimal digits only"));
    }
    if provenance.producer_attempt == 0 {
        return Err(invalid("Plan producerAttempt must be greater than zero"));
    }
    if !provenance
        .planning_job
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
    {
        return Err(invalid(
            "Plan planningJob has characters unsafe for an artifact name",
        ));
    }
    if !is_sha(&provenance.base) || !is_sha(&provenance.head) {
        return Err(invalid(
            "Plan base and head must be full 40-character SHA-1 values",
        ));
    }
    Ok(())
}

fn validate_artifact_digest(artifact: &ArtifactDigest) -> Result<()> {
    if artifact.name.trim().is_empty() || !is_digest(&artifact.sha256) {
        return Err(invalid(
            "Plan artifact reference needs a name and SHA-256 digest",
        ));
    }
    Ok(())
}

fn validate_sorted_paths(group: &str, paths: &[String]) -> Result<()> {
    let mut prior: Option<&str> = None;
    for path in paths {
        if !valid_relative_path(path) || prior.is_some_and(|previous| previous >= path.as_str()) {
            return Err(invalid(format!(
                "Plan assignment in group '{group}' has unsafe, duplicate, or unsorted checkout paths"
            )));
        }
        prior = Some(path);
    }
    if paths.is_empty() {
        return Err(invalid(format!(
            "Plan assignment in group '{group}' has no checkout paths"
        )));
    }
    Ok(())
}

fn validate_assignment_prediction(
    preparation_ms: Option<u64>,
    preparation_source: Option<&str>,
    preparation_samples: Option<usize>,
    scheduling_mode: Option<&str>,
) -> Result<()> {
    if !matches!(scheduling_mode, None | Some("automatic") | Some("cold-cap")) {
        return Err(invalid("Plan assignment has an unknown schedulingMode"));
    }
    match (preparation_ms, preparation_source, preparation_samples) {
        (None, None, None) => Ok(()),
        (Some(_), Some("exact" | "group"), Some(samples)) if samples > 0 => Ok(()),
        _ => Err(invalid(
            "Plan assignment has incomplete preparation prediction diagnostics",
        )),
    }
}

fn valid_relative_path(path: &str) -> bool {
    if path == "." {
        return true;
    }
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn artifact_name(provenance: &PlanProvenance) -> Result<String> {
    validate_provenance(provenance)?;
    Ok(format!(
        "nanoom-plan-v1-{}-{}-{}",
        provenance.run_id, provenance.producer_attempt, provenance.planning_job
    ))
}

fn same_sha(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::ConfigValidation(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> PlanProvenance {
        PlanProvenance {
            repository: "owner/repo".into(),
            workflow: ".github/workflows/ci.yml@refs/heads/main".into(),
            run_id: "12345".into(),
            producer_attempt: 1,
            planning_job: "affected".into(),
            base: "a".repeat(40),
            head: "b".repeat(40),
        }
    }

    fn plan_with_items(item_count: usize, assignment_count: usize) -> Plan {
        let mut groups = BTreeMap::new();
        let mut assignments = Vec::new();
        for assignment_index in 0..assignment_count {
            let start = item_count * assignment_index / assignment_count;
            let end = item_count * (assignment_index + 1) / assignment_count;
            let items: Vec<_> = (start..end)
                .map(|index| PlanItem {
                    group: "ci".into(),
                    name: format!("pkg-{index:05}"),
                    path: format!("packages/pkg-{index:05}"),
                    task: "test".into(),
                    shard: None,
                    total_shards: None,
                })
                .collect();
            assignments.push(PlanAssignment {
                assignment_id: format!("ci-{assignment_index:04}"),
                checkout_paths: items.iter().map(|item| item.path.clone()).collect(),
                items,
                predicted_duration_ms: Some(1),
                predicted_preparation_ms: None,
                prediction_sources: None,
                preparation_prediction_source: None,
                preparation_sample_count: None,
                scheduling_mode: None,
                prediction_reason: "cold".into(),
                runner_labels: Some(vec!["ubuntu-latest".into()]),
                timing_environment: Some("ubuntu-24.04-x64".into()),
            });
        }
        groups.insert("ci".into(), PlanGroup { assignments });
        Plan {
            version: PLAN_VERSION,
            provenance: provenance(),
            task_runner: "pnpm".into(),
            prediction_reason: "cold start".into(),
            prediction_artifact: None,
            model_artifact: None,
            has_change: item_count > 0,
            groups,
            assignment_count,
            item_count,
        }
    }

    #[test]
    fn large_plan_keeps_compact_matrix_and_output_bounded() {
        let plan = plan_with_items(10_000, 24);
        plan.validate().unwrap();
        let bytes = serde_json::to_vec_pretty(&plan).unwrap();
        assert!(
            bytes.len() > 1_000_000,
            "fixture should exercise a large plan"
        );
        let reference = PlanReference::for_plan(&plan, &bytes).unwrap();
        let compact = compact_output(&plan, &reference).unwrap();
        let value: Value = serde_json::from_str(&compact).unwrap();
        assert_eq!(
            value["groups"]["ci"]["include"].as_array().unwrap().len(),
            24
        );
        assert!(compact.encode_utf16().count() * 2 <= MAX_OUTPUT_UTF16_BYTES);
        assert_eq!(value["result"]["itemCount"], 10_000);
    }

    #[test]
    fn compact_matrix_rejects_more_than_256_rows() {
        let plan = plan_with_items(257, 257);
        let bytes = serde_json::to_vec(&plan).unwrap();
        let reference = PlanReference::for_plan(&plan, &bytes).unwrap();
        let error = compact_output(&plan, &reference).unwrap_err().to_string();
        assert!(error.contains("maximum is 256"));
    }

    #[test]
    fn compact_matrix_keeps_preparation_prediction_diagnostics() {
        let mut plan = plan_with_items(1, 1);
        let assignment = &mut plan.groups.get_mut("ci").unwrap().assignments[0];
        assignment.predicted_preparation_ms = Some(250);
        assignment.preparation_prediction_source = Some("group".into());
        assignment.preparation_sample_count = Some(7);
        assignment.scheduling_mode = Some("automatic".into());
        assignment.prediction_reason =
            "automatic concurrency: selected 1 assignments by preparation + task makespan".into();
        plan.validate().unwrap();
        let bytes = serde_json::to_vec_pretty(&plan).unwrap();
        let reference = PlanReference::for_plan(&plan, &bytes).unwrap();
        let compact: Value =
            serde_json::from_str(&compact_output(&plan, &reference).unwrap()).unwrap();
        let row = &compact["groups"]["ci"]["include"][0];
        assert_eq!(row["predictedPreparationMs"], 250);
        assert_eq!(row["preparationPredictionSource"], "group");
        assert_eq!(row["preparationSampleCount"], 7);
        assert_eq!(row["schedulingMode"], "automatic");
        assert!(row["predictionReason"]
            .as_str()
            .unwrap()
            .contains("selected 1"));
    }

    #[test]
    fn reference_allows_same_run_retry_and_rejects_identity_or_head_mismatch() {
        let plan = plan_with_items(1, 1);
        let bytes = serde_json::to_vec_pretty(&plan).unwrap();
        let mut reference = PlanReference::for_plan(&plan, &bytes).unwrap();
        reference.current.attempt = 2;
        reference.validate(&plan, &bytes).unwrap();

        reference.current.attempt = 0;
        assert!(reference.validate(&plan, &bytes).is_err());
        reference.current.attempt = 2;

        reference.current.run_id = "54321".into();
        assert!(reference.validate(&plan, &bytes).is_err());
        reference.current.run_id = plan.provenance.run_id.clone();
        reference.current.head = "c".repeat(40);
        assert!(reference.validate(&plan, &bytes).is_err());
    }

    #[test]
    fn plan_validation_rejects_empty_assignment_and_duplicate_items() {
        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0]
            .items
            .clear();
        plan.item_count = 0;
        plan.has_change = false;
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("no items"));

        let mut plan = plan_with_items(2, 2);
        let duplicate = plan.groups["ci"].assignments[0].items[0].clone();
        plan.groups.get_mut("ci").unwrap().assignments[1].items[0] = duplicate;
        assert!(plan.validate().is_err());
    }

    #[test]
    fn valid_zero_work_plan_has_no_assignments_and_bounded_output() {
        let plan = plan_with_items(0, 0);
        plan.validate().unwrap();
        let bytes = serde_json::to_vec_pretty(&plan).unwrap();
        let reference = PlanReference::for_plan(&plan, &bytes).unwrap();
        let value: Value =
            serde_json::from_str(&compact_output(&plan, &reference).unwrap()).unwrap();
        assert_eq!(value["has_change"], false);
        assert_eq!(value["groups"]["ci"]["include"], serde_json::json!([]));
        assert_eq!(value["result"]["assignmentCount"], 0);
    }

    #[test]
    fn rejects_unsafe_checkout_paths() {
        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0].checkout_paths = vec!["../escape".into()];
        assert!(plan.validate().is_err());
    }

    #[test]
    fn plan_validation_rejects_invalid_provenance_predictions_and_counts() {
        let mut value = provenance();
        value.repository.clear();
        assert!(validate_provenance(&value).is_err());
        let mut value = provenance();
        value.workflow.push('\n');
        assert!(validate_provenance(&value).is_err());
        let mut value = provenance();
        value.run_id = "12x".into();
        assert!(validate_provenance(&value).is_err());
        let mut value = provenance();
        value.producer_attempt = 0;
        assert!(validate_provenance(&value).is_err());
        let mut value = provenance();
        value.planning_job = "job/name".into();
        assert!(validate_provenance(&value).is_err());
        let mut value = provenance();
        value.head = "short".into();
        assert!(validate_provenance(&value).is_err());

        let mut plan = plan_with_items(1, 1);
        plan.version += 1;
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unsupported Plan version"));
        let mut plan = plan_with_items(1, 1);
        plan.task_runner.clear();
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("taskRunner"));
        let mut plan = plan_with_items(1, 1);
        plan.prediction_artifact = Some(ArtifactDigest {
            name: "prediction".into(),
            sha256: "bad".into(),
        });
        assert!(plan.validate().unwrap_err().to_string().contains("SHA-256"));
        let mut plan = plan_with_items(1, 1);
        plan.groups.insert(" ".into(), plan.groups["ci"].clone());
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("group names"));
        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0]
            .assignment_id
            .clear();
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("assignmentId"));
        let mut plan = plan_with_items(2, 2);
        let duplicate_id = plan.groups["ci"].assignments[0].assignment_id.clone();
        plan.groups.get_mut("ci").unwrap().assignments[1].assignment_id = duplicate_id;
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("assignmentId"));
        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0].predicted_preparation_ms = Some(1);
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("preparation prediction"));
        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0].scheduling_mode = Some("guess".into());
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("schedulingMode"));
        let mut plan = plan_with_items(1, 1);
        plan.assignment_count += 1;
        assert!(plan.validate().unwrap_err().to_string().contains("counts"));
        let mut plan = plan_with_items(1, 1);
        plan.has_change = false;
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("hasChange"));
    }

    #[test]
    fn plan_validation_rejects_invalid_items_shards_and_checkout_paths() {
        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0]
            .checkout_paths
            .clear();
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("no checkout paths"));

        let mutate_item: [fn(&mut PlanItem); 4] = [
            |item: &mut PlanItem| item.group = "other".into(),
            |item: &mut PlanItem| item.name.clear(),
            |item: &mut PlanItem| item.task.clear(),
            |item: &mut PlanItem| item.path = "../escape".into(),
        ];
        for mutate in mutate_item {
            let mut plan = plan_with_items(1, 1);
            mutate(&mut plan.groups.get_mut("ci").unwrap().assignments[0].items[0]);
            assert!(plan.validate().is_err());
        }

        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0].items[0].path = "packages/other".into();
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("not in checkout paths"));
        let mut plan = plan_with_items(1, 1);
        plan.groups.get_mut("ci").unwrap().assignments[0].items[0].shard = Some(1);
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("shard layout"));
        let mut plan = plan_with_items(2, 2);
        let duplicate = plan.groups["ci"].assignments[0].items[0].clone();
        plan.groups.get_mut("ci").unwrap().assignments[1].items[0] = duplicate;
        plan.groups.get_mut("ci").unwrap().assignments[1].checkout_paths =
            vec!["packages/pkg-00000".into()];
        assert!(plan
            .validate()
            .unwrap_err()
            .to_string()
            .contains("assigned more than once"));

        for paths in [
            vec!["packages/pkg-00000".into(), "packages/pkg-00000".into()],
            vec!["packages/z".into(), "packages/a".into()],
        ] {
            let mut plan = plan_with_items(1, 1);
            plan.groups.get_mut("ci").unwrap().assignments[0].checkout_paths = paths;
            assert!(plan
                .validate()
                .unwrap_err()
                .to_string()
                .contains("unsorted checkout paths"));
        }
    }

    #[test]
    fn reference_validation_rejects_digest_artifact_current_run_and_provenance_mismatches() {
        let plan = plan_with_items(1, 1);
        let bytes = serde_json::to_vec_pretty(&plan).unwrap();
        let base = PlanReference::for_plan(&plan, &bytes).unwrap();

        let mut reference = base.clone();
        reference.version += 1;
        assert!(reference
            .validate(&plan, &bytes)
            .unwrap_err()
            .to_string()
            .contains("reference version"));
        let mut reference = base.clone();
        reference.sha256 = "0".repeat(64);
        assert!(reference
            .validate(&plan, &bytes)
            .unwrap_err()
            .to_string()
            .contains("SHA-256"));
        let mut reference = base.clone();
        reference.artifact_name.push_str("-other");
        assert!(reference
            .validate(&plan, &bytes)
            .unwrap_err()
            .to_string()
            .contains("artifact name"));
        let mut reference = base.clone();
        reference.current.workflow.push_str("-other");
        assert!(reference
            .validate(&plan, &bytes)
            .unwrap_err()
            .to_string()
            .contains("different repository"));
        let mut reference = base.clone();
        reference.current.attempt = 0;
        assert!(reference
            .validate(&plan, &bytes)
            .unwrap_err()
            .to_string()
            .contains("newer"));
        let mut reference = base;
        reference.provenance.run_id = "54321".into();
        reference.current.run_id = "54321".into();
        reference.artifact_name = artifact_name(&reference.provenance).unwrap();
        assert!(reference
            .validate(&plan, &bytes)
            .unwrap_err()
            .to_string()
            .contains("provenance"));
    }

    #[test]
    fn matrix_assignment_parser_validates_fields_paths_and_optional_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let row = serde_json::json!({
            "assignmentId": "ci-1",
            "items": [{"name":"pkg","path":"packages/pkg","task":"test","shard":1,"totalShards":2}],
            "checkout": {"sparseCheckout":"packages/pkg"},
            "predictedDurationMs": 42,
            "predictedPreparationMs": 10,
            "preparationPredictionSource": "exact",
            "preparationSampleCount": 3,
            "schedulingMode": "automatic",
            "reason": "warm",
            "runnerLabels": ["ubuntu-latest"],
            "timingEnvironment": "linux-x64"
        });
        let assignment = assignment_from_matrix("ci", 0, &row, dir.path(), &root).unwrap();
        assert_eq!(assignment.assignment_id, "ci-1");
        assert_eq!(assignment.items[0].total_shards, Some(2));
        assert_eq!(assignment.predicted_preparation_ms, Some(10));

        let legacy = serde_json::json!({
            "name":"pkg","path":dir.path().join("packages/pkg"),"task":"test",
            "checkout":{"sparseCheckout":dir.path().join("packages/pkg").to_string_lossy()}
        });
        let legacy = assignment_from_matrix("ci", 1, &legacy, dir.path(), &root).unwrap();
        assert_eq!(legacy.assignment_id, "ci-0002");
        assert_eq!(legacy.checkout_paths, ["packages/pkg"]);

        let invalid_rows = [
            serde_json::json!({"items":[],"checkout":{"sparseCheckout":"packages/pkg"}}),
            serde_json::json!({"assignmentId":"ci-1","items":[{"path":"packages/pkg","task":"test"}],"checkout":{"sparseCheckout":"packages/pkg"}}),
            serde_json::json!({"assignmentId":"ci-1","items":[{"name":"pkg","path":"../escape","task":"test"}],"checkout":{"sparseCheckout":"packages/pkg"}}),
            serde_json::json!({"assignmentId":"ci-1","items":[{"name":"pkg","path":"packages/pkg","task":"test","shard":"one"}],"checkout":{"sparseCheckout":"packages/pkg"}}),
            serde_json::json!({"assignmentId":"ci-1","items":[{"name":"pkg","path":"packages/pkg","task":"test"}],"checkout":{}}),
            serde_json::json!({"assignmentId":"ci-1","items":[{"name":"pkg","path":"packages/pkg","task":"test"}],"checkout":{"sparseCheckout":"../escape"}}),
            serde_json::json!({"assignmentId":"ci-1","items":[{"name":"pkg","path":"packages/pkg","task":"test"}],"checkout":{"sparseCheckout":"packages/pkg"},"runnerLabels":"ubuntu-latest"}),
        ];
        for row in invalid_rows {
            assert!(
                assignment_from_matrix("ci", 0, &row, dir.path(), &root).is_err(),
                "{row}"
            );
        }
        let outside = serde_json::json!({
            "assignmentId":"ci-1",
            "items":[{"name":"pkg","path":"/tmp/outside","task":"test"}],
            "checkout":{"sparseCheckout":"packages/pkg"}
        });
        assert!(assignment_from_matrix("ci", 0, &outside, dir.path(), &root).is_err());
    }

    #[test]
    fn context_and_path_helpers_reject_untrusted_values() {
        let base_context = PlanContext {
            repository: "owner/repo".into(),
            workflow: ".github/workflows/ci.yml@refs/heads/main".into(),
            run_id: "123".into(),
            producer_attempt: 1,
            planning_job: "affected".into(),
            base: "a".repeat(40),
            head: "b".repeat(40),
            task_runner: "pnpm".into(),
            prediction_reason: "cold".into(),
            prediction_artifact: None,
            model_artifact: None,
        };
        base_context.validate().unwrap();
        let mut context = base_context.clone();
        context.task_runner.clear();
        assert!(context.validate().is_err());
        let mut context = base_context.clone();
        context.model_artifact = Some(ArtifactDigest {
            name: "model".into(),
            sha256: "bad".into(),
        });
        assert!(context.validate().is_err());

        for value in [
            "/outside",
            "../escape",
            "packages/../escape",
            "packages\\escape",
        ] {
            assert!(
                workspace_relative_path(value, Path::new("/repo"), Path::new("/repo")).is_err()
            );
        }
        assert_eq!(
            workspace_relative_path(".", Path::new("/repo"), Path::new("/repo")).unwrap(),
            "."
        );
        assert_eq!(
            workspace_relative_path("/repo/packages/pkg", Path::new("/repo"), Path::new("/repo"))
                .unwrap(),
            "packages/pkg"
        );
        assert!(optional_usize(&serde_json::json!({"shard":"one"}), "shard").is_err());
        assert_eq!(
            optional_usize(&serde_json::json!({"shard":2}), "shard").unwrap(),
            Some(2)
        );
        assert!(validate_sorted_paths("ci", &[]).is_err());
    }
}
