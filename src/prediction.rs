use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

pub const VERSION: u8 = 3;
pub const DAY_MS: u64 = 86_400_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const MAX_DURATION_MS: u64 = 604_800_000;
const FUTURE_TOLERANCE_MS: u64 = 300_000;
const BATCH_MAX_AGE_MS: u64 = 7 * DAY_MS;
const RECEIPT_RETENTION_MS: u64 = 8 * DAY_MS;
const BUCKET_HORIZON_DAYS: u64 = 30;
const MAX_BUCKETS: usize = 7;
const MAX_KEYS: usize = 50_000;
const MAX_BATCH_COUNT: u64 = 50_000;
const MAX_RECEIPTS: usize = 4096;
const MAX_MODEL_BYTES: usize = 16 * 1024 * 1024;
const MAX_PREDICTION_BYTES: usize = 8 * 1024 * 1024;
const MAX_MEASUREMENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ScopeRef {
    Push {
        #[serde(rename = "ref")]
        git_ref: String,
    },
    PullRequest {
        number: u64,
        head_repository_id: String,
        head_ref: String,
        base_ref: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub repository_key: String,
    pub workflow_path: String,
    #[serde(rename = "ref")]
    pub git_ref: ScopeRef,
    pub group: String,
    pub task_runner: String,
    pub timing_environment: String,
}

impl Scope {
    pub fn validate(&self) -> Result<(), String> {
        if self.repository_key.is_empty()
            || self.repository_key.len() > 64
            || !self
                .repository_key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            || !self
                .repository_key
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err("invalid repositoryKey".into());
        }
        let file = self
            .workflow_path
            .strip_prefix(".github/workflows/")
            .ok_or("workflowPath must be under .github/workflows")?;
        if file.is_empty()
            || file.contains('/')
            || file == "."
            || file == ".."
            || !(file.ends_with(".yml") || file.ends_with(".yaml"))
            || has_control(&self.workflow_path)
        {
            return Err("invalid workflowPath".into());
        }
        for (field, value, max) in [
            ("group", self.group.as_str(), 128),
            ("taskRunner", self.task_runner.as_str(), 32),
            ("timingEnvironment", self.timing_environment.as_str(), 256),
        ] {
            if value.is_empty() || value.len() > max || has_control(value) {
                return Err(format!("invalid {field}"));
            }
        }
        if !matches!(
            self.task_runner.as_str(),
            "pnpm" | "yarn" | "npm" | "nx" | "turbo"
        ) {
            return Err("unsupported taskRunner".into());
        }
        match &self.git_ref {
            ScopeRef::Push { git_ref } => validate_branch_ref(git_ref)?,
            ScopeRef::PullRequest {
                number,
                head_repository_id,
                head_ref,
                base_ref,
            } => {
                if *number == 0
                    || *number > MAX_SAFE_INTEGER
                    || head_repository_id.is_empty()
                    || head_repository_id.len() > 20
                    || !head_repository_id.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err("invalid pull request identity".into());
                }
                validate_branch_ref(head_ref)?;
                validate_branch_ref(base_ref)?;
            }
        }
        Ok(())
    }

    pub fn id(&self) -> Result<String, String> {
        self.validate()?;
        digest_value(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PredictionContext {
    pub repository_key: String,
    pub workflow_path: String,
    #[serde(rename = "ref")]
    pub git_ref: ScopeRef,
}

impl PredictionContext {
    pub fn validate(&self) -> Result<(), String> {
        self.scope(self.git_ref.clone(), "context", "npm", "context")
            .validate()
    }

    pub fn scope(
        &self,
        git_ref: ScopeRef,
        group: &str,
        task_runner: &str,
        environment: &str,
    ) -> Scope {
        Scope {
            repository_key: self.repository_key.clone(),
            workflow_path: self.workflow_path.clone(),
            git_ref,
            group: group.into(),
            task_runner: task_runner.into(),
            timing_environment: environment.into(),
        }
    }

    fn candidates(&self, group: &str, task_runner: &str, environment: &str) -> Vec<Scope> {
        let mut scopes = vec![self.scope(self.git_ref.clone(), group, task_runner, environment)];
        if let ScopeRef::PullRequest { base_ref, .. } = &self.git_ref {
            scopes.push(self.scope(
                ScopeRef::Push {
                    git_ref: base_ref.clone(),
                },
                group,
                task_runner,
                environment,
            ));
        }
        scopes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeasurementArtifact {
    pub version: u8,
    pub scope: Scope,
    pub run_id: String,
    pub run_attempt: u32,
    pub observations: Vec<SuccessfulObservation>,
}

impl MeasurementArtifact {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let bytes = read_bounded(path, MAX_MEASUREMENT_BYTES)?;
        let artifact: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION
            || !valid_positive_decimal(&self.run_id)
            || self.run_attempt == 0
            || self.observations.is_empty()
        {
            return Err("invalid or empty v3 MeasurementArtifact".into());
        }
        self.scope.validate()?;
        if self.observations.len() > MAX_BATCH_COUNT as usize {
            return Err("MeasurementArtifact exceeds 50000 observations".into());
        }
        let produced_at_ms = self
            .observations
            .iter()
            .map(|observation| observation.observed_at_ms)
            .max()
            .ok_or("MeasurementArtifact has no observations")?;
        for observation in &self.observations {
            validate_observation(&self.scope, observation, produced_at_ms)?;
        }
        if canonical_bytes(self)?.len() > MAX_MEASUREMENT_BYTES {
            return Err("MeasurementArtifact exceeds 16 MiB".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactReference {
    pub name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PredictionArtifact {
    pub table: PredictionTable,
    pub model_artifact: ArtifactReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PredictionArtifactBundle {
    pub version: u8,
    pub predictions: Vec<PredictionArtifact>,
}

impl PredictionArtifactBundle {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let bytes = read_bounded(path, MAX_PREDICTION_BYTES)?;
        if bytes.len() > MAX_PREDICTION_BYTES {
            return Err("PredictionArtifact exceeds 8 MiB".into());
        }
        let bundle: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION || self.predictions.len() > MAX_KEYS {
            return Err("invalid PredictionArtifact version or table count".into());
        }
        let mut scopes = BTreeSet::new();
        for artifact in &self.predictions {
            artifact.table.validate()?;
            if artifact.model_artifact.name.is_empty()
                || artifact.model_artifact.name.len() > 256
                || has_control(&artifact.model_artifact.name)
            {
                return Err("invalid model artifact reference name".into());
            }
            validate_digest(&artifact.model_artifact.sha256)?;
            if !scopes.insert(artifact.table.scope.id()?) {
                return Err("PredictionArtifact contains duplicate scopes".into());
            }
        }
        if canonical_bytes(self)?.len() > MAX_PREDICTION_BYTES {
            return Err("PredictionArtifact exceeds 8 MiB".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PredictionIndex {
    tables: BTreeMap<String, PredictionTable>,
}

impl PredictionIndex {
    pub fn new(
        bundles: impl IntoIterator<Item = PredictionArtifactBundle>,
    ) -> Result<Self, String> {
        let mut tables = BTreeMap::new();
        for bundle in bundles {
            for artifact in bundle.predictions {
                let scope_id = artifact.table.scope.id()?;
                if tables.insert(scope_id, artifact.table).is_some() {
                    return Err("duplicate PredictionTable scope".into());
                }
            }
        }
        Ok(Self { tables })
    }

    pub fn estimate(
        &self,
        context: &PredictionContext,
        group: &str,
        task_runner: &str,
        environment: &str,
        key_id: &str,
        now_ms: u64,
    ) -> Option<(u64, u64)> {
        context
            .candidates(group, task_runner, environment)
            .into_iter()
            .filter_map(|scope| scope.id().ok().map(|scope_id| (scope, scope_id)))
            .find_map(|(scope, scope_id)| {
                self.tables
                    .get(&scope_id)
                    .filter(|table| table.scope == scope)
                    .and_then(|table| table.estimate(key_id, now_ms))
            })
    }

    pub fn has_valid_rows(&self, now_ms: u64) -> bool {
        self.tables
            .values()
            .any(|table| table.rows.iter().any(|row| now_ms < row.valid_until_ms()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PredictionTable {
    pub version: u8,
    pub scope: Scope,
    pub model_updated_at_ms: u64,
    pub rows: Vec<PredictionRow>,
}

impl PredictionTable {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("unsupported PredictionTable version".into());
        }
        self.scope.validate()?;
        if self.rows.len() > MAX_KEYS {
            return Err("PredictionTable exceeds 50000 rows".into());
        }
        if self.model_updated_at_ms > MAX_SAFE_INTEGER {
            return Err("invalid PredictionTable modelUpdatedAtMs".into());
        }
        let mut previous = None;
        for row in &self.rows {
            row.validate()?;
            if previous.as_deref().is_some_and(|key| key >= row.key_id()) {
                return Err("PredictionTable rows must be unique and sorted by keyId".into());
            }
            previous = Some(row.key_id().to_owned());
        }
        if canonical_bytes(self)?.len() > MAX_PREDICTION_BYTES {
            return Err("PredictionTable exceeds 8 MiB".into());
        }
        Ok(())
    }

    pub fn estimate(&self, key_id: &str, now_ms: u64) -> Option<(u64, u64)> {
        self.rows
            .binary_search_by(|row| row.key_id().cmp(key_id))
            .ok()
            .map(|index| &self.rows[index])
            .filter(|row| now_ms < row.valid_until_ms())
            .map(|row| (row.estimated_ms(), row.observation_count()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PredictionRow(pub String, pub u64, pub u64, pub u64, pub u64);

impl PredictionRow {
    pub fn key_id(&self) -> &str {
        &self.0
    }
    pub fn estimated_ms(&self) -> u64 {
        self.1
    }
    pub fn observation_count(&self) -> u64 {
        self.2
    }
    pub fn last_observed_at_ms(&self) -> u64 {
        self.3
    }
    pub fn valid_until_ms(&self) -> u64 {
        self.4
    }
    fn validate(&self) -> Result<(), String> {
        validate_digest(&self.0)?;
        if self.1 > MAX_SAFE_INTEGER
            || self.2 == 0
            || self.2 > MAX_SAFE_INTEGER
            || self.3 > MAX_SAFE_INTEGER
            || self.4 > MAX_SAFE_INTEGER
            || self.4 <= self.3
        {
            return Err("invalid or expired PredictionTable row".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PredictionKey {
    TaskExact {
        group: String,
        workspace: String,
        task: String,
        shard: Option<u64>,
        #[serde(rename = "totalShards")]
        total_shards: Option<u64>,
        #[serde(rename = "taskRunner")]
        task_runner: String,
        #[serde(rename = "timingEnvironment")]
        timing_environment: String,
    },
    TaskFallback {
        group: String,
        task: String,
        shard: Option<u64>,
        #[serde(rename = "totalShards")]
        total_shards: Option<u64>,
        #[serde(rename = "taskRunner")]
        task_runner: String,
        #[serde(rename = "timingEnvironment")]
        timing_environment: String,
    },
    PreparationExact {
        group: String,
        #[serde(rename = "taskRunner")]
        task_runner: String,
        #[serde(rename = "timingEnvironment")]
        timing_environment: String,
        #[serde(rename = "packageManager")]
        package_manager: String,
        #[serde(rename = "packageManagerVersion")]
        package_manager_version: String,
        #[serde(rename = "installMode")]
        install_mode: String,
        #[serde(rename = "lockfileDigest")]
        lockfile_digest: String,
        #[serde(rename = "checkoutDigest")]
        checkout_digest: String,
        #[serde(rename = "workspaceSetDigest")]
        workspace_set_digest: String,
    },
    PreparationFallback {
        group: String,
        #[serde(rename = "taskRunner")]
        task_runner: String,
        #[serde(rename = "timingEnvironment")]
        timing_environment: String,
        #[serde(rename = "packageManager")]
        package_manager: String,
        #[serde(rename = "packageManagerVersion")]
        package_manager_version: String,
        #[serde(rename = "installMode")]
        install_mode: String,
        #[serde(rename = "lockfileDigest")]
        lockfile_digest: String,
    },
}

impl PredictionKey {
    pub fn id(&self) -> Result<String, String> {
        digest_value(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SuccessfulObservation {
    pub execution_id: String,
    pub observed_at_ms: u64,
    pub group: String,
    pub workspace: String,
    pub task: String,
    pub shard: Option<u64>,
    pub total_shards: Option<u64>,
    pub task_runner: String,
    pub timing_environment: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AggregateObservation {
    pub key_id: String,
    pub utc_epoch_day: u64,
    pub observation_count: u64,
    pub total_duration_ms: u64,
    pub last_observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservationBatch {
    pub version: u8,
    pub scope: Scope,
    pub batch_id: String,
    pub run_id: String,
    pub run_attempt: u32,
    pub produced_at_ms: u64,
    pub aggregates: Vec<AggregateObservation>,
}

impl ObservationBatch {
    pub fn validate(&self, now_ms: u64) -> Result<(), String> {
        if self.version != VERSION || !valid_positive_decimal(&self.run_id) || self.run_attempt == 0
        {
            return Err("invalid ObservationBatch identity or version".into());
        }
        self.scope.validate()?;
        if self.produced_at_ms > now_ms.saturating_add(FUTURE_TOLERANCE_MS)
            || now_ms.saturating_sub(self.produced_at_ms) >= BATCH_MAX_AGE_MS
        {
            return Err("ObservationBatch is expired or from the future".into());
        }
        let expected_id = batch_id(&self.scope, &self.run_id, self.run_attempt)?;
        if self.batch_id != expected_id {
            return Err("ObservationBatch batchId does not match scope/run/attempt".into());
        }
        if self.aggregates.is_empty() || self.aggregates.len() > MAX_KEYS {
            return Err("ObservationBatch aggregate count is outside limits".into());
        }
        let mut previous: Option<(&str, u64)> = None;
        for aggregate in &self.aggregates {
            validate_aggregate(aggregate, now_ms)?;
            let key = (aggregate.key_id.as_str(), aggregate.utc_epoch_day);
            if previous.is_some_and(|value| value >= key) {
                return Err(
                    "aggregates must have unique key/day rows sorted by keyId and day".into(),
                );
            }
            previous = Some(key);
        }
        if canonical_bytes(self)?.len() > 16 * 1024 * 1024 {
            return Err("ObservationBatch exceeds 16 MiB".into());
        }
        Ok(())
    }

    pub fn body_digest(&self) -> Result<String, String> {
        digest_value(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelState {
    pub version: u8,
    pub scope: Scope,
    pub updated_at_ms: u64,
    pub pruning_day: u64,
    pub batch_acceptance_after_ms: u64,
    pub entries: Vec<ModelEntry>,
    pub receipts: Vec<BatchReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelStateBundle {
    pub version: u8,
    pub states: Vec<ModelState>,
}

impl ModelStateBundle {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let bytes = read_bounded(path, MAX_MODEL_BYTES)?;
        let bundle: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION || self.states.len() > MAX_KEYS {
            return Err("invalid ModelStateBundle version or state count".into());
        }
        let mut scopes = BTreeSet::new();
        for state in &self.states {
            validate_model(state)?;
            if !scopes.insert(state.scope.id()?) {
                return Err("ModelStateBundle contains duplicate scopes".into());
            }
        }
        if canonical_bytes(self)?.len() > MAX_MODEL_BYTES {
            return Err("ModelStateBundle exceeds 16 MiB".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelEntry {
    pub key_id: String,
    pub buckets: Vec<DayBucket>,
    pub prediction: ModelPrediction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DayBucket(pub u64, pub u64, pub u64, pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelPrediction(pub u64, pub u64, pub u64, pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchReceipt(pub String, pub String, pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied { pruned_buckets: u64 },
    Duplicate,
}

pub fn compile_batch(
    scope: Scope,
    run_id: String,
    run_attempt: u32,
    produced_at_ms: u64,
    observations: &[SuccessfulObservation],
) -> Result<ObservationBatch, String> {
    scope.validate()?;
    if !valid_positive_decimal(&run_id) || run_attempt == 0 {
        return Err("invalid run identity".into());
    }
    let mut unique: BTreeMap<String, &SuccessfulObservation> = BTreeMap::new();
    for observation in observations {
        validate_observation(&scope, observation, produced_at_ms)?;
        match unique.get(observation.execution_id.as_str()) {
            Some(previous) if **previous != *observation => {
                return Err("one executionId has conflicting observations".into())
            }
            Some(_) => continue,
            None => {
                unique.insert(observation.execution_id.clone(), observation);
            }
        }
    }
    let mut aggregates: BTreeMap<(String, u64), (u64, u64, u64)> = BTreeMap::new();
    for observation in unique.values() {
        let exact = PredictionKey::TaskExact {
            group: observation.group.clone(),
            workspace: observation.workspace.clone(),
            task: observation.task.clone(),
            shard: observation.shard,
            total_shards: observation.total_shards,
            task_runner: observation.task_runner.clone(),
            timing_environment: observation.timing_environment.clone(),
        }
        .id()?;
        let fallback = PredictionKey::TaskFallback {
            group: observation.group.clone(),
            task: observation.task.clone(),
            shard: observation.shard,
            total_shards: observation.total_shards,
            task_runner: observation.task_runner.clone(),
            timing_environment: observation.timing_environment.clone(),
        }
        .id()?;
        let day = observation.observed_at_ms / DAY_MS;
        for key_id in [exact, fallback] {
            let value = aggregates.entry((key_id, day)).or_default();
            value.0 = value.0.checked_add(1).ok_or("observation count overflow")?;
            value.1 = value
                .1
                .checked_add(observation.duration_ms)
                .ok_or("duration sum overflow")?;
            value.2 = value.2.max(observation.observed_at_ms);
            if value.0 > MAX_BATCH_COUNT || value.1 > MAX_SAFE_INTEGER {
                return Err("compiled aggregate exceeds count or safe integer limit".into());
            }
        }
    }
    if aggregates.is_empty() {
        return Err("cannot compile an empty observation batch".into());
    }
    let batch = ObservationBatch {
        version: VERSION,
        batch_id: batch_id(&scope, &run_id, run_attempt)?,
        scope,
        run_id,
        run_attempt,
        produced_at_ms,
        aggregates: aggregates
            .into_iter()
            .map(
                |(
                    (key_id, utc_epoch_day),
                    (observation_count, total_duration_ms, last_observed_at_ms),
                )| {
                    AggregateObservation {
                        key_id,
                        utc_epoch_day,
                        observation_count,
                        total_duration_ms,
                        last_observed_at_ms,
                    }
                },
            )
            .collect(),
    };
    batch.validate(produced_at_ms)?;
    Ok(batch)
}

pub fn apply_batch(
    previous: Option<ModelState>,
    batch: &ObservationBatch,
    now_ms: u64,
) -> Result<(ModelState, ApplyOutcome), String> {
    batch.validate(now_ms)?;
    let body_digest = batch.body_digest()?;
    let today = now_ms / DAY_MS;
    let acceptance_after = now_ms.saturating_sub(BATCH_MAX_AGE_MS).saturating_add(1);
    let mut state = previous.unwrap_or_else(|| ModelState {
        version: VERSION,
        scope: batch.scope.clone(),
        updated_at_ms: now_ms,
        pruning_day: today,
        batch_acceptance_after_ms: acceptance_after,
        entries: Vec::new(),
        receipts: Vec::new(),
    });
    validate_model(&state)?;
    if state.scope != batch.scope {
        return Err("ObservationBatch scope does not match ModelState".into());
    }
    if let Some(receipt) = state
        .receipts
        .iter()
        .find(|receipt| receipt.0 == batch.batch_id)
    {
        if receipt.1 == body_digest {
            return Ok((state, ApplyOutcome::Duplicate));
        }
        return Err("batchId already exists with a different body digest".into());
    }
    if batch.produced_at_ms < state.batch_acceptance_after_ms {
        return Err("ObservationBatch is older than the model acceptance watermark".into());
    }

    let effective_today = batch
        .aggregates
        .iter()
        .map(|aggregate| aggregate.utc_epoch_day)
        .max()
        .unwrap_or(today)
        .max(today)
        .max(state.pruning_day);
    let cutoff_day = effective_today.saturating_sub(BUCKET_HORIZON_DAYS - 1);

    let mut pruned_buckets = 0_u64;
    let mut by_key: BTreeMap<String, BTreeMap<u64, DayBucket>> = BTreeMap::new();
    for entry in state.entries.drain(..) {
        for bucket in entry.buckets {
            if bucket.0 >= cutoff_day && bucket.0 <= effective_today {
                by_key
                    .entry(entry.key_id.clone())
                    .or_default()
                    .insert(bucket.0, bucket);
            } else {
                pruned_buckets = pruned_buckets.saturating_add(1);
            }
        }
    }
    for aggregate in &batch.aggregates {
        let day = aggregate.utc_epoch_day;
        if day < cutoff_day || day > effective_today {
            continue;
        }
        let buckets = by_key.entry(aggregate.key_id.clone()).or_default();
        let bucket =
            buckets
                .entry(day)
                .or_insert(DayBucket(day, 0, 0, aggregate.last_observed_at_ms));
        bucket.1 = bucket
            .1
            .checked_add(aggregate.observation_count)
            .filter(|value| *value <= MAX_SAFE_INTEGER)
            .ok_or("model observation count overflow")?;
        bucket.2 = bucket
            .2
            .checked_add(aggregate.total_duration_ms)
            .filter(|value| *value <= MAX_SAFE_INTEGER)
            .ok_or("model duration sum overflow")?;
        bucket.3 = bucket.3.max(aggregate.last_observed_at_ms);
    }

    let mut entries = Vec::new();
    for (key_id, buckets) in by_key {
        let mut buckets: Vec<DayBucket> = buckets.into_values().collect();
        if buckets.len() > MAX_BUCKETS {
            let drop_count = buckets.len() - MAX_BUCKETS;
            pruned_buckets = pruned_buckets.saturating_add(drop_count as u64);
            buckets.drain(..drop_count);
        }
        if let Some(prediction) = derive_prediction(&buckets) {
            entries.push(ModelEntry {
                key_id,
                buckets,
                prediction,
            });
        }
    }
    if entries.len() > MAX_KEYS {
        return Err("ModelState exceeds 50000 keys".into());
    }
    let mut receipts: Vec<BatchReceipt> = state
        .receipts
        .into_iter()
        .filter(|receipt| now_ms.saturating_sub(receipt.2) < RECEIPT_RETENTION_MS)
        .collect();
    if receipts.len() >= MAX_RECEIPTS {
        return Err("ModelState receipt capacity exceeded".into());
    }
    receipts.push(BatchReceipt(
        batch.batch_id.clone(),
        body_digest,
        batch.produced_at_ms,
    ));
    receipts.sort_by(|a, b| a.0.cmp(&b.0));
    state.version = VERSION;
    state.updated_at_ms = state.updated_at_ms.max(now_ms);
    state.pruning_day = effective_today;
    state.batch_acceptance_after_ms = state.batch_acceptance_after_ms.max(acceptance_after);
    state.entries = entries;
    state.receipts = receipts;
    validate_model(&state)?;
    let bytes = canonical_bytes(&state)?;
    if bytes.len() > MAX_MODEL_BYTES {
        return Err("ModelState exceeds 16 MiB".into());
    }
    Ok((state, ApplyOutcome::Applied { pruned_buckets }))
}

pub fn project_predictions(state: &ModelState, now_ms: u64) -> Result<PredictionTable, String> {
    validate_model(state)?;
    let today = now_ms / DAY_MS;
    let cutoff_day = today.saturating_sub(BUCKET_HORIZON_DAYS - 1);
    let mut rows = Vec::new();
    for entry in &state.entries {
        let buckets: Vec<DayBucket> = entry
            .buckets
            .iter()
            .filter(|bucket| bucket.0 >= cutoff_day && bucket.0 <= today)
            .cloned()
            .collect();
        let Some(prediction) = derive_prediction(&buckets) else {
            continue;
        };
        if now_ms >= prediction.3 {
            continue;
        }
        rows.push(PredictionRow(
            entry.key_id.clone(),
            prediction.0,
            prediction.1,
            prediction.2,
            prediction.3,
        ));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let table = PredictionTable {
        version: VERSION,
        scope: state.scope.clone(),
        model_updated_at_ms: state.updated_at_ms,
        rows,
    };
    if canonical_bytes(&table)?.len() > MAX_PREDICTION_BYTES {
        return Err("PredictionTable exceeds 8 MiB".into());
    }
    Ok(table)
}

pub fn validate_model(state: &ModelState) -> Result<(), String> {
    if state.version != VERSION {
        return Err("unsupported ModelState version".into());
    }
    state.scope.validate()?;
    if state.updated_at_ms > MAX_SAFE_INTEGER
        || state.pruning_day > 3_652_059
        || state.batch_acceptance_after_ms > MAX_SAFE_INTEGER
        || state.entries.len() > MAX_KEYS
        || state.receipts.len() > MAX_RECEIPTS
    {
        return Err("ModelState exceeds a field or collection limit".into());
    }
    let mut previous_key: Option<&str> = None;
    for entry in &state.entries {
        validate_digest(&entry.key_id)?;
        if previous_key.is_some_and(|key| key >= entry.key_id.as_str()) {
            return Err("ModelState entries must be unique and sorted by keyId".into());
        }
        previous_key = Some(&entry.key_id);
        if entry.buckets.is_empty() || entry.buckets.len() > MAX_BUCKETS {
            return Err("ModelState bucket count is outside limits".into());
        }
        let mut previous_day = None;
        for bucket in &entry.buckets {
            validate_bucket(bucket)?;
            if previous_day.is_some_and(|day| day >= bucket.0) {
                return Err("ModelState buckets must be unique and sorted by day".into());
            }
            previous_day = Some(bucket.0);
        }
        if entry.buckets.iter().any(|bucket| {
            bucket.0 > state.pruning_day
                || bucket.0 < state.pruning_day.saturating_sub(BUCKET_HORIZON_DAYS - 1)
        }) {
            return Err("ModelState bucket is outside its pruning horizon".into());
        }
        if derive_prediction(&entry.buckets).as_ref() != Some(&entry.prediction) {
            return Err("ModelState cached prediction does not match its buckets".into());
        }
    }
    let mut previous_receipt: Option<&str> = None;
    for receipt in &state.receipts {
        validate_digest(&receipt.0)?;
        validate_digest(&receipt.1)?;
        if receipt.2 > MAX_SAFE_INTEGER
            || previous_receipt.is_some_and(|id| id >= receipt.0.as_str())
        {
            return Err("ModelState receipts must be valid, unique and sorted by batchId".into());
        }
        previous_receipt = Some(&receipt.0);
    }
    if canonical_bytes(state)?.len() > MAX_MODEL_BYTES {
        return Err("ModelState exceeds 16 MiB".into());
    }
    Ok(())
}

pub fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    serde_json::to_vec(&value).map_err(|error| error.to_string())
}

pub fn digest_value<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(hex_digest(&canonical_bytes(value)?))
}

pub fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn batch_id(scope: &Scope, run_id: &str, run_attempt: u32) -> Result<String, String> {
    let scope_id = scope.id()?;
    let value = serde_json::json!([scope_id, run_id, run_attempt]);
    digest_value(&value)
}

fn derive_prediction(buckets: &[DayBucket]) -> Option<ModelPrediction> {
    let latest_day = buckets.last()?.0;
    let count = buckets.iter().try_fold(0_u64, |sum, bucket| {
        sum.checked_add(bucket.1)
            .filter(|count| *count <= MAX_SAFE_INTEGER)
    })?;
    let last_observed = buckets.iter().map(|bucket| bucket.3).max()?;
    let mut numerator = 0.0_f64;
    let mut denominator = 0.0_f64;
    for bucket in buckets {
        let weight = 2.0_f64.powf(-((latest_day - bucket.0) as f64) / 7.0);
        numerator += bucket.2 as f64 * weight;
        denominator += bucket.1 as f64 * weight;
    }
    if denominator == 0.0 {
        return None;
    }
    let estimated = (numerator / denominator + 0.5).floor();
    if !estimated.is_finite() || estimated < 0.0 || estimated > MAX_SAFE_INTEGER as f64 {
        return None;
    }
    let valid_until = buckets
        .first()?
        .0
        .saturating_add(BUCKET_HORIZON_DAYS)
        .checked_mul(DAY_MS)?;
    Some(ModelPrediction(
        estimated as u64,
        count,
        last_observed,
        valid_until,
    ))
}

fn validate_observation(
    scope: &Scope,
    observation: &SuccessfulObservation,
    produced_at_ms: u64,
) -> Result<(), String> {
    if observation.execution_id.is_empty()
        || observation.execution_id.len() > 1024
        || has_control(&observation.execution_id)
        || observation.group != scope.group
        || observation.task_runner != scope.task_runner
        || observation.timing_environment != scope.timing_environment
        || observation.workspace.is_empty()
        || observation.task.is_empty()
        || has_control(&observation.workspace)
        || has_control(&observation.task)
        || observation.duration_ms == 0
        || observation.duration_ms > MAX_DURATION_MS
        || observation.observed_at_ms > MAX_SAFE_INTEGER
        || observation.observed_at_ms > produced_at_ms.saturating_add(FUTURE_TOLERANCE_MS)
        || produced_at_ms.saturating_sub(observation.observed_at_ms) >= BATCH_MAX_AGE_MS
    {
        return Err("invalid or expired successful observation".into());
    }
    match (observation.shard, observation.total_shards) {
        (None, None) => {}
        (Some(shard), Some(total)) if shard > 0 && shard <= total && total <= MAX_SAFE_INTEGER => {}
        _ => return Err("invalid shard identity".into()),
    }
    Ok(())
}

fn validate_aggregate(aggregate: &AggregateObservation, now_ms: u64) -> Result<(), String> {
    validate_digest(&aggregate.key_id)?;
    let day = aggregate.utc_epoch_day;
    if day > 3_652_059
        || aggregate.observation_count == 0
        || aggregate.observation_count > MAX_BATCH_COUNT
        || aggregate.total_duration_ms > MAX_SAFE_INTEGER
        || aggregate.total_duration_ms > aggregate.observation_count.saturating_mul(MAX_DURATION_MS)
        || aggregate.last_observed_at_ms > MAX_SAFE_INTEGER
        || aggregate.last_observed_at_ms > now_ms.saturating_add(FUTURE_TOLERANCE_MS)
        || aggregate.last_observed_at_ms / DAY_MS != day
    {
        return Err("invalid aggregate row".into());
    }
    Ok(())
}

fn validate_bucket(bucket: &DayBucket) -> Result<(), String> {
    if bucket.0 > 3_652_059
        || bucket.1 == 0
        || bucket.1 > MAX_SAFE_INTEGER
        || bucket.2 > MAX_SAFE_INTEGER
        || bucket.2 > bucket.1.saturating_mul(MAX_DURATION_MS)
        || bucket.3 > MAX_SAFE_INTEGER
        || bucket.3 / DAY_MS != bucket.0
    {
        return Err("invalid model day bucket".into());
    }
    Ok(())
}

fn validate_branch_ref(value: &str) -> Result<(), String> {
    if !value.starts_with("refs/heads/")
        || value.len() <= "refs/heads/".len()
        || value.len() > 512
        || has_control(value)
        || value.contains(' ')
        || value.contains("..")
        || value.contains("@{")
        || value.ends_with('/')
        || value.ends_with('.')
        || value.ends_with(".lock")
        || value
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with('.'))
    {
        return Err("invalid Git branch ref".into());
    }
    Ok(())
}

fn validate_digest(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("digest must be lowercase SHA-256 hex".into());
    }
    Ok(())
}

fn valid_positive_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.as_bytes()[0] != b'0'
}

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn read_bounded(path: &std::path::Path, max_bytes: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > max_bytes {
        return Err(format!(
            "artifact exceeds {} MiB",
            max_bytes / (1024 * 1024)
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope {
            repository_key: "github-12345".into(),
            workflow_path: ".github/workflows/ci.yml".into(),
            git_ref: ScopeRef::Push {
                git_ref: "refs/heads/main".into(),
            },
            group: "ci".into(),
            task_runner: "yarn".into(),
            timing_environment: "linux-x64-node24".into(),
        }
    }

    fn observation(id: &str, at: u64, duration: u64) -> SuccessfulObservation {
        SuccessfulObservation {
            execution_id: id.into(),
            observed_at_ms: at,
            group: "ci".into(),
            workspace: "@fixture/web".into(),
            task: "test".into(),
            shard: None,
            total_shards: None,
            task_runner: "yarn".into(),
            timing_environment: "linux-x64-node24".into(),
            duration_ms: duration,
        }
    }

    #[test]
    fn matches_openapi_jcs_scope_key_and_batch_vectors() {
        let scope = scope();
        assert_eq!(
            scope.id().unwrap(),
            "b351ab1307a793d7499f196a59cab4707a1535ce6a8470285a8b9e3384a74a47"
        );
        let key = PredictionKey::TaskExact {
            group: "ci".into(),
            workspace: "@fixture/web".into(),
            task: "test".into(),
            shard: None,
            total_shards: None,
            task_runner: "yarn".into(),
            timing_environment: "linux-x64-node24".into(),
        };
        assert_eq!(
            key.id().unwrap(),
            "8e3a060d9912f5f4230ab9f0a25df7fe50c4d9814cca14ec6f20f795a7e0fcda"
        );
        assert_eq!(
            batch_id(&scope, "123456789", 1).unwrap(),
            "1368e6bbb03a78c15d423d23afac569f4f8bfd6e1b10a38b9f4c8e647177b9b5"
        );
    }

    #[test]
    fn compiler_deduplicates_execution_ids_and_sorts_aggregate_rows() {
        let at = 1_790_208_000_000;
        let rows = [
            observation("exec-b", at, 40),
            observation("exec-a", at, 20),
            observation("exec-a", at, 20),
        ];
        let batch = compile_batch(scope(), "123456789".into(), 1, at, &rows).unwrap();
        assert_eq!(batch.aggregates.len(), 2);
        assert_eq!(batch.aggregates[0].observation_count, 2);
        assert_eq!(batch.aggregates[0].total_duration_ms, 60);
        assert!(batch.aggregates.windows(2).all(|pair| (
            pair[0].key_id.as_str(),
            pair[0].utc_epoch_day
        ) < (
            pair[1].key_id.as_str(),
            pair[1].utc_epoch_day
        )));
    }

    #[test]
    fn apply_is_idempotent_and_projection_is_expiry_bounded() {
        let at = 1_790_208_000_000;
        let batch = compile_batch(
            scope(),
            "123456789".into(),
            1,
            at,
            &[observation("exec-a", at, 12_000)],
        )
        .unwrap();
        let (state, outcome) = apply_batch(None, &batch, at).unwrap();
        assert_eq!(outcome, ApplyOutcome::Applied { pruned_buckets: 0 });
        let (same, outcome) = apply_batch(Some(state.clone()), &batch, at + 1).unwrap();
        assert_eq!(outcome, ApplyOutcome::Duplicate);
        assert_eq!(same, state);
        let table = project_predictions(&state, at).unwrap();
        assert_eq!(
            table.rows[0],
            PredictionRow(
                batch.aggregates[0].key_id.clone(),
                12_000,
                1,
                at,
                (batch.aggregates[0].utc_epoch_day + 30) * DAY_MS
            )
        );
        table.validate().unwrap();
        assert!(
            project_predictions(&state, (batch.aggregates[0].utc_epoch_day + 30) * DAY_MS)
                .unwrap()
                .rows
                .is_empty()
        );
    }

    #[test]
    fn changed_batch_body_and_live_receipt_capacity_are_rejected_atomically() {
        let at = 1_790_208_000_000;
        let batch = compile_batch(
            scope(),
            "123456789".into(),
            1,
            at,
            &[observation("exec-a", at, 12_000)],
        )
        .unwrap();
        let (state, _) = apply_batch(None, &batch, at).unwrap();
        let mut changed = batch.clone();
        changed.aggregates[0].total_duration_ms += 1;
        assert!(apply_batch(Some(state.clone()), &changed, at + 1).is_err());

        let next_batch = compile_batch(
            scope(),
            "123456790".into(),
            1,
            at + DAY_MS,
            &[observation("exec-b", at + DAY_MS, 9_000)],
        )
        .unwrap();
        let mut full_state = state.clone();
        full_state.receipts = (0..MAX_RECEIPTS)
            .map(|index| BatchReceipt(format!("{index:064x}"), format!("{:064x}", index + 1), at))
            .collect();
        full_state.receipts.sort_by(|a, b| a.0.cmp(&b.0));
        assert!(apply_batch(Some(full_state.clone()), &next_batch, at + DAY_MS).is_err());

        for receipt in &mut full_state.receipts {
            receipt.2 = at.saturating_sub(RECEIPT_RETENTION_MS);
        }
        let (pruned, outcome) = apply_batch(Some(full_state), &next_batch, at + DAY_MS).unwrap();
        assert_eq!(outcome, ApplyOutcome::Applied { pruned_buckets: 0 });
        assert_eq!(pruned.receipts.len(), 1);
    }

    #[test]
    fn pruned_model_does_not_resurrect_buckets_after_clock_rollback() {
        let first_at = 20_000 * DAY_MS;
        let first = compile_batch(
            scope(),
            "123456789".into(),
            1,
            first_at,
            &[observation("first", first_at, 10)],
        )
        .unwrap();
        let (state, _) = apply_batch(None, &first, first_at).unwrap();

        let later_at = first_at + 31 * DAY_MS;
        let later = compile_batch(
            scope(),
            "123456790".into(),
            1,
            later_at,
            &[observation("later", later_at, 20)],
        )
        .unwrap();
        let (state, _) = apply_batch(Some(state), &later, later_at).unwrap();
        assert!(state.entries.iter().all(|entry| entry
            .buckets
            .iter()
            .all(|bucket| bucket.0 >= later_at / DAY_MS - (BUCKET_HORIZON_DAYS - 1))));

        let rollback_at = later_at - DAY_MS;
        let rollback = compile_batch(
            scope(),
            "123456791".into(),
            1,
            rollback_at,
            &[observation("rollback", rollback_at, 30)],
        )
        .unwrap();
        let (state, _) = apply_batch(Some(state), &rollback, rollback_at).unwrap();
        assert_eq!(state.pruning_day, later_at / DAY_MS);
        assert!(state.entries.iter().all(|entry| entry
            .buckets
            .iter()
            .all(|bucket| bucket.0 >= state.pruning_day - (BUCKET_HORIZON_DAYS - 1))));
    }

    #[test]
    fn same_observations_in_different_orders_project_to_same_bytes() {
        let at = 1_790_208_000_000;
        let observations = [observation("exec-a", at, 20), observation("exec-b", at, 30)];
        let forward = compile_batch(scope(), "123456789".into(), 1, at, &observations).unwrap();
        let reverse = compile_batch(
            scope(),
            "123456789".into(),
            1,
            at,
            &observations.into_iter().rev().collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(
            canonical_bytes(&forward).unwrap(),
            canonical_bytes(&reverse).unwrap()
        );

        let (forward, _) = apply_batch(None, &forward, at).unwrap();
        let (reverse, _) = apply_batch(None, &reverse, at).unwrap();
        assert_eq!(
            canonical_bytes(&project_predictions(&forward, at).unwrap()).unwrap(),
            canonical_bytes(&project_predictions(&reverse, at).unwrap()).unwrap()
        );
    }

    #[test]
    fn prediction_rows_reject_invalid_expiry_and_unsafe_counts() {
        let mut row = PredictionRow("0".repeat(64), 1, 1, 100, 100);
        assert!(row.validate().is_err());
        row.4 = 101;
        assert!(row.validate().is_ok());
        assert!(derive_prediction(&[
            DayBucket(10, MAX_SAFE_INTEGER, 1, 10 * DAY_MS),
            DayBucket(11, 1, 1, 11 * DAY_MS),
        ])
        .is_none());
    }

    #[test]
    fn pr_scope_isolated_from_base_push_scope() {
        let mut pr = scope();
        pr.git_ref = ScopeRef::PullRequest {
            number: 42,
            head_repository_id: "98765".into(),
            head_ref: "refs/heads/feature".into(),
            base_ref: "refs/heads/main".into(),
        };
        pr.validate().unwrap();
        assert_ne!(pr.id().unwrap(), scope().id().unwrap());
    }

    #[test]
    fn conflicting_execution_and_duplicate_aggregate_rows_are_rejected() {
        let at = 1_790_208_000_000;
        let rows = [observation("same", at, 1), observation("same", at, 2)];
        assert!(compile_batch(scope(), "123456789".into(), 1, at, &rows).is_err());
        let mut batch = compile_batch(
            scope(),
            "123456789".into(),
            1,
            at,
            &[observation("one", at, 1)],
        )
        .unwrap();
        batch.aggregates.push(batch.aggregates[0].clone());
        assert!(batch.validate(at).is_err());
    }

    #[test]
    fn one_hundred_thousand_observations_remain_seven_buckets() {
        let start = 1_790_208_000_000;
        let mut state = None;
        for batch_index in 0..2 {
            let at = start + batch_index * DAY_MS;
            let rows: Vec<_> = (0..50_000)
                .map(|index| observation(&format!("{batch_index}-{index}"), at, 1))
                .collect();
            let batch = compile_batch(
                scope(),
                format!("{}", 123_456_789 + batch_index),
                1,
                at,
                &rows,
            )
            .unwrap();
            let (next, _) = apply_batch(state, &batch, at).unwrap();
            state = Some(next);
        }
        let state = state.unwrap();
        assert_eq!(state.entries.len(), 2);
        assert!(state.entries.iter().all(|entry| entry.buckets.len() == 2
            && entry.buckets.iter().map(|bucket| bucket.1).sum::<u64>() == 100_000));
        assert!(validate_model(&state).is_ok());
    }

    #[test]
    fn weighted_estimate_uses_daily_aggregates_not_raw_samples() {
        let older_day = 20_000;
        let buckets = vec![
            DayBucket(older_day, 2, 200, older_day * DAY_MS),
            DayBucket(older_day + 7, 1, 100, (older_day + 7) * DAY_MS),
        ];
        let prediction = derive_prediction(&buckets).unwrap();
        assert_eq!(prediction.0, 100);
        assert_eq!(prediction.1, 3);
        assert_eq!(prediction.2, (older_day + 7) * DAY_MS);
    }
}
