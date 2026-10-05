use nanoom::prediction::{
    digest_value, project_predictions, validate_model, ModelState, ModelStateBundle,
    PredictionArtifact, PredictionArtifactBundle, PredictionTable,
};

#[test]
fn published_prediction_examples_pass_runtime_validation() {
    let api: serde_json::Value =
        serde_yaml::from_str(include_str!("../docs/api/history.openapi.yaml")).unwrap();
    let schemas = &api["components"]["schemas"];
    let vectors = &api["x-contract-examples"];
    let model: ModelState =
        serde_json::from_value(schemas["ModelState"]["examples"][0].clone()).unwrap();
    validate_model(&model).unwrap();
    assert_eq!(digest_value(&model).unwrap(), vectors["modelDigest"]);
    let model_bundle: ModelStateBundle =
        serde_json::from_value(schemas["ModelStateBundle"]["examples"][0].clone()).unwrap();
    model_bundle.validate().unwrap();
    assert_eq!(
        digest_value(&model_bundle).unwrap(),
        vectors["modelBundleDigest"]
    );
    let projection = project_predictions(&model, vectors["clockMs"].as_u64().unwrap()).unwrap();
    for value in schemas["PredictionArtifactEntry"]["examples"]
        .as_array()
        .unwrap()
    {
        let entry: PredictionArtifact = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(entry.table, projection);
        assert_eq!(
            entry.model_artifact.sha256,
            digest_value(&model_bundle).unwrap()
        );
        PredictionArtifactBundle {
            version: 3,
            predictions: vec![entry],
        }
        .validate()
        .unwrap();
    }
    for value in schemas["PredictionArtifact"]["examples"]
        .as_array()
        .unwrap()
    {
        let bundle: PredictionArtifactBundle = serde_json::from_value(value.clone()).unwrap();
        bundle.validate().unwrap();
        assert_eq!(bundle.predictions[0].table, projection);
        assert_eq!(
            bundle.predictions[0].model_artifact.sha256,
            digest_value(&model_bundle).unwrap()
        );
    }
    let path = "/v1/repositories/{repositoryKey}/scopes/{scopeId}/snapshot";
    let table: PredictionTable = serde_json::from_value(
        api["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["example"]
            .clone(),
    )
    .unwrap();
    table.validate().unwrap();
    assert_eq!(table, projection);
}
