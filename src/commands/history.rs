use crate::error::Result;
use crate::prediction::{
    apply_batch, canonical_bytes, compile_batch_with_preparation, digest_value, hex_digest,
    project_predictions, ApplyOutcome, MeasurementArtifact, ModelStateBundle, PredictionArtifact,
    PredictionArtifactBundle, Scope, VERSION,
};
use clap::Args;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct HistoryArgs {
    #[arg(
        long = "input",
        help = "Successful MeasurementArtifact v3 JSON (repeatable)"
    )]
    pub inputs: Vec<PathBuf>,

    #[arg(long, help = "Previous ModelStateBundle v3 JSON")]
    pub previous_model: Option<PathBuf>,

    #[arg(
        long,
        help = "Previous PredictionArtifact v3 JSON that publishes the model"
    )]
    pub previous_prediction: Option<PathBuf>,

    #[arg(
        long,
        help = "Previous model artifact name used by the prior prediction marker"
    )]
    pub previous_model_name: Option<String>,

    #[arg(long, required = true, help = "New ModelStateBundle v3 output path")]
    pub model_output: PathBuf,

    #[arg(
        long,
        required = true,
        help = "New PredictionArtifact v3 publish-marker output path"
    )]
    pub prediction_output: PathBuf,

    #[arg(
        long,
        required = true,
        help = "Name used when uploading the model artifact"
    )]
    pub model_artifact_name: String,

    #[arg(long, required = true, help = "Current GitHub Actions run ID")]
    pub run_id: String,

    #[arg(long, required = true, help = "Current GitHub Actions run attempt")]
    pub run_attempt: u32,

    #[arg(
        long,
        hide = true,
        help = "Override current UTC milliseconds for reproducible checks"
    )]
    pub now_ms: Option<u64>,
}

pub fn execute(args: HistoryArgs) -> Result<()> {
    if args.previous_model.is_some() != args.previous_prediction.is_some()
        || args.previous_model.is_some() != args.previous_model_name.is_some()
    {
        return Err(crate::error::Error::InvalidConfig(
            "--previous-model, --previous-prediction, and --previous-model-name must be supplied together".into(),
        ));
    }
    let now_ms = args.now_ms.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    });
    let (mut states, previous_model_degraded) = load_previous_states(&args)?;
    let mut measurements: BTreeMap<String, (Scope, Vec<_>, Vec<_>)> = BTreeMap::new();
    let mut rejected_measurement_count = 0_u64;
    for path in &args.inputs {
        let measurement = match MeasurementArtifact::load(path) {
            Ok(measurement)
                if measurement.run_id == args.run_id
                    && measurement.run_attempt == args.run_attempt =>
            {
                measurement
            }
            Ok(_) => {
                eprintln!(
                    "measurement run identity mismatch; skipping {}",
                    path.display()
                );
                rejected_measurement_count += 1;
                continue;
            }
            Err(error) => {
                eprintln!(
                    "MeasurementArtifact v3 unavailable; skipping {}: {error}",
                    path.display()
                );
                rejected_measurement_count += 1;
                continue;
            }
        };
        let scope_id = measurement
            .scope
            .id()
            .map_err(crate::error::Error::InvalidConfig)?;
        let (scope, observations, preparation_observations) = measurements
            .entry(scope_id)
            .or_insert_with(|| (measurement.scope.clone(), Vec::new(), Vec::new()));
        if *scope != measurement.scope {
            return Err(crate::error::Error::InvalidConfig(
                "scope ID collision while grouping measurements".into(),
            ));
        }
        observations.extend(measurement.observations);
        preparation_observations.extend(measurement.preparation_observations);
    }

    let mut applied_batch_count = 0_u64;
    let mut duplicate_batch_count = 0_u64;
    let mut degraded_scope_count = 0_u64;
    let mut accepted_observation_count = 0_u64;
    for (scope_id, (scope, observations, preparation_observations)) in measurements {
        let produced_at_ms = observations
            .iter()
            .map(|observation| observation.observed_at_ms)
            .chain(
                preparation_observations
                    .iter()
                    .map(|observation| observation.observed_at_ms),
            )
            .max()
            .unwrap_or(now_ms);
        let batch = match compile_batch_with_preparation(
            scope.clone(),
            args.run_id.clone(),
            args.run_attempt,
            produced_at_ms,
            &observations,
            &preparation_observations,
        ) {
            Ok(batch) => batch,
            Err(error) => {
                eprintln!("could not compile scope {scope_id}; retaining prior state: {error}");
                degraded_scope_count += 1;
                continue;
            }
        };
        let unique_task_observations = observations
            .iter()
            .map(|observation| observation.execution_id.as_str())
            .collect::<BTreeSet<_>>()
            .len() as u64;
        let unique_preparation_observations = preparation_observations
            .iter()
            .map(|observation| observation.execution_id.as_str())
            .collect::<BTreeSet<_>>()
            .len() as u64;
        let unique_observations = unique_task_observations + unique_preparation_observations;

        match apply_batch(states.get(&scope_id).cloned(), &batch, now_ms) {
            Ok((state, ApplyOutcome::Applied { .. })) => {
                states.insert(scope_id, state);
                applied_batch_count += 1;
                accepted_observation_count += unique_observations;
            }
            Ok((state, ApplyOutcome::Duplicate)) => {
                states.insert(scope_id, state);
                duplicate_batch_count += 1;
                accepted_observation_count += unique_observations;
            }
            Err(error) => {
                eprintln!("could not apply scope {scope_id}; retaining prior state: {error}");
                degraded_scope_count += 1;
            }
        }
    }

    let mut state_bundle = ModelStateBundle {
        version: VERSION,
        states: states.into_values().collect(),
    };
    state_bundle
        .states
        .sort_by_key(|state| state.scope.id().unwrap_or_default());
    state_bundle
        .validate()
        .map_err(crate::error::Error::InvalidConfig)?;
    let model_bytes = canonical_bytes(&state_bundle).map_err(crate::error::Error::InvalidConfig)?;
    write_output(&args.model_output, &model_bytes)?;
    let model_digest = hex_digest(&model_bytes);

    let predictions = state_bundle
        .states
        .iter()
        .map(|state| {
            let table =
                project_predictions(state, now_ms).map_err(crate::error::Error::InvalidConfig)?;
            Ok(PredictionArtifact {
                table,
                model_artifact: crate::prediction::ArtifactReference {
                    name: args.model_artifact_name.clone(),
                    sha256: model_digest.clone(),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let prediction_bundle = PredictionArtifactBundle {
        version: VERSION,
        predictions,
    };
    prediction_bundle
        .validate()
        .map_err(crate::error::Error::InvalidConfig)?;
    let prediction_bytes =
        canonical_bytes(&prediction_bundle).map_err(crate::error::Error::InvalidConfig)?;
    write_output(&args.prediction_output, &prediction_bytes)?;

    let status =
        if rejected_measurement_count > 0 || degraded_scope_count > 0 || previous_model_degraded {
            "degraded"
        } else {
            "success"
        };
    println!(
        "{}",
        serde_json::json!({
            "status": status,
            "runId": args.run_id,
            "runAttempt": args.run_attempt,
            "measurementFiles": args.inputs.len(),
            "rejectedMeasurementFiles": rejected_measurement_count,
            "previousModelDegraded": previous_model_degraded,
            "scopeCount": state_bundle.states.len(),
            "appliedBatchCount": applied_batch_count,
            "duplicateBatchCount": duplicate_batch_count,
            "degradedScopeCount": degraded_scope_count,
            "acceptedObservationCount": accepted_observation_count,
            "modelArtifactName": args.model_artifact_name,
            "modelArtifactSha256": model_digest,
            "modelBytes": model_bytes.len(),
            "predictionBytes": prediction_bytes.len()
        })
    );
    Ok(())
}

fn load_previous_states(
    args: &HistoryArgs,
) -> Result<(BTreeMap<String, crate::prediction::ModelState>, bool)> {
    let (Some(model_path), Some(prediction_path), Some(model_name)) = (
        args.previous_model.as_ref(),
        args.previous_prediction.as_ref(),
        args.previous_model_name.as_ref(),
    ) else {
        return Ok((BTreeMap::new(), false));
    };
    let model = match ModelStateBundle::load(model_path) {
        Ok(model) => model,
        Err(error) => {
            eprintln!("previous ModelStateBundle unavailable; bootstrapping from current observations: {error}");
            return Ok((BTreeMap::new(), true));
        }
    };
    let prediction = match PredictionArtifactBundle::load(prediction_path) {
        Ok(prediction) => prediction,
        Err(error) => {
            eprintln!("previous PredictionArtifact unavailable; bootstrapping from current observations: {error}");
            return Ok((BTreeMap::new(), true));
        }
    };
    let digest = digest_value(&model).map_err(crate::error::Error::InvalidConfig)?;
    if prediction.predictions.is_empty()
        || prediction.predictions.iter().any(|artifact| {
            artifact.model_artifact.name != *model_name || artifact.model_artifact.sha256 != digest
        })
    {
        eprintln!("previous model artifact pointer did not match; bootstrapping from current observations");
        return Ok((BTreeMap::new(), true));
    }
    Ok((
        model
            .states
            .into_iter()
            .map(|state| {
                let id = state.scope.id().unwrap_or_default();
                (id, state)
            })
            .collect(),
        false,
    ))
}

fn write_output(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}
