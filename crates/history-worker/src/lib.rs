mod auth;
mod json;

use auth::{Authorization, Authorizer, Permission};
use futures_util::StreamExt;
use nanoom_prediction_core::{
    apply_batch, canonical_bytes, hex_digest, project_predictions, validate_model, ApplyOutcome,
    ModelState, ObservationBatch,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use worker::{
    event, Context, D1Database, D1Type, Date, Delay, Env, Headers, Method, Request, Response,
    ScheduleContext, ScheduledEvent,
};

const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const MAX_MODEL_BYTES: usize = 1_900_000;
const CAS_ATTEMPTS: usize = 8;

#[derive(Debug)]
struct ApiError {
    status: u16,
    code: &'static str,
    title: &'static str,
    detail: &'static str,
    authenticate: bool,
    allow: Option<&'static str>,
}

impl ApiError {
    fn new(status: u16, code: &'static str, title: &'static str, detail: &'static str) -> Self {
        Self {
            status,
            code,
            title,
            detail,
            authenticate: false,
            allow: None,
        }
    }

    fn unauthorized() -> Self {
        Self {
            status: 401,
            code: "unauthenticated",
            title: "Unauthorized",
            detail: "Missing or invalid bearer credential.",
            authenticate: true,
            allow: None,
        }
    }

    fn method_not_allowed(allow: &'static str) -> Self {
        let mut error = Self::new(
            405,
            "method_not_allowed",
            "Method Not Allowed",
            "The method is not allowed for this endpoint.",
        );
        error.allow = Some(allow);
        error
    }

    fn storage() -> Self {
        Self::new(
            503,
            "storage_unavailable",
            "Service Unavailable",
            "History storage is temporarily unavailable.",
        )
    }

    fn corrupt() -> Self {
        Self::new(
            503,
            "storage_corrupt",
            "Service Unavailable",
            "Stored history is invalid and needs operator recovery.",
        )
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeResult {
    scope_id: String,
    model_version: u8,
    outcome: &'static str,
    applied: bool,
    received_aggregate_count: usize,
    retained_key_count: usize,
    pruned_bucket_count: u64,
    receipt_count: usize,
    model_updated_at_ms: u64,
}

#[event(fetch)]
pub async fn main(mut request: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    let request_id = request
        .headers()
        .get("cf-ray")?
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .unwrap_or_else(|| format!("local-{}", Date::now().as_millis()));
    let result = handle(&mut request, &env, &request_id).await;
    match result {
        Ok(response) => Ok(response),
        Err(error) => problem_response(error, &request_id),
    }
}

async fn handle(request: &mut Request, env: &Env, request_id: &str) -> Result<Response, ApiError> {
    let path = request.path();
    match (request.method(), path.as_str()) {
        (Method::Get, "/health") => {
            return json_response(
                200,
                &serde_json::json!({"status":"ok"}),
                request_id,
                "no-store",
            )
        }
        (Method::Get, "/ready") => {
            let ready = match history_database(env) {
                Ok(database) => database
                    .prepare("SELECT 1 AS ready")
                    .first::<i32>(Some("ready"))
                    .await
                    .ok()
                    .flatten()
                    .is_some(),
                Err(_) => false,
            };
            if ready {
                return json_response(
                    200,
                    &serde_json::json!({"status":"ready"}),
                    request_id,
                    "no-store",
                );
            }
            return json_response(
                503,
                &serde_json::json!({
                    "status":"not_ready",
                    "reason":"storage_unavailable",
                    "requestId": request_id,
                }),
                request_id,
                "no-store",
            );
        }
        _ => {}
    }

    let (repository_key, scope_id, operation) = parse_history_path(&path).ok_or_else(|| {
        ApiError::new(
            404,
            "not_found",
            "Not Found",
            "No endpoint matches this path.",
        )
    })?;
    let permission = match (request.method(), operation) {
        (Method::Get, Operation::Snapshot) => Permission::Read,
        (Method::Post, Operation::Merge) => Permission::Write,
        (_, Operation::Snapshot) => return Err(ApiError::method_not_allowed("GET")),
        (_, Operation::Merge) => return Err(ApiError::method_not_allowed("POST")),
    };
    let authorizer = load_authorizer(env)?;
    let bearer = bearer_token(request)?;
    match authorizer.authorize(&bearer, repository_key, scope_id, permission) {
        Authorization::Allowed(_principal_id) => {}
        Authorization::InvalidCredential => return Err(ApiError::unauthorized()),
        Authorization::Forbidden => {
            return Err(ApiError::new(
                403,
                "forbidden",
                "Forbidden",
                "Principal lacks exact scope permission.",
            ));
        }
    }
    let database = history_database(env)?;
    let key = model_key(repository_key, scope_id);

    match operation {
        Operation::Snapshot => {
            get_snapshot(request, database, key, repository_key, scope_id, request_id).await
        }
        Operation::Merge => {
            merge_observations(request, database, key, repository_key, scope_id, request_id).await
        }
    }
}

#[event(scheduled)]
pub async fn cleanup_stale_history(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let Ok(database) = history_database(&env) else {
        return;
    };
    let cutoff = D1Type::Real(Date::now().as_millis() as f64 - 45.0 * 24.0 * 60.0 * 60.0 * 1000.0);
    // ponytail: delete at most 10k stale rows per day; a larger backlog drains over multiple days.
    let Ok(statement) = database
        .prepare(
            "DELETE FROM prediction_state WHERE storage_key IN (SELECT storage_key FROM prediction_state WHERE updated_at_ms < ?1 ORDER BY updated_at_ms LIMIT 10000)",
        )
        .bind_refs([&cutoff])
    else {
        return;
    };
    if statement.run().await.is_err() {
        worker::console_error!("Scheduled history cleanup failed.");
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Snapshot,
    Merge,
}

fn parse_history_path(path: &str) -> Option<(&str, &str, Operation)> {
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 7
        || !parts[0].is_empty()
        || parts[1] != "v1"
        || parts[2] != "repositories"
        || parts[4] != "scopes"
    {
        return None;
    }
    let repository_key = parts[3];
    let scope_id = parts[5];
    if !auth::valid_repository_key(repository_key) || !auth::valid_scope_id(scope_id) {
        return None;
    }
    let operation = match parts[6] {
        "snapshot" => Operation::Snapshot,
        "observations:merge" => Operation::Merge,
        _ => return None,
    };
    Some((repository_key, scope_id, operation))
}

fn load_authorizer(env: &Env) -> Result<Authorizer, ApiError> {
    let config = env.secret("NANOOM_AUTH_JSON").map_err(|_| {
        ApiError::new(
            503,
            "configuration_error",
            "Service Unavailable",
            "History authorization is not configured.",
        )
    })?;
    Authorizer::from_json(&config.to_string()).map_err(|_| {
        ApiError::new(
            503,
            "configuration_error",
            "Service Unavailable",
            "History authorization configuration is invalid.",
        )
    })
}

fn bearer_token(request: &Request) -> Result<String, ApiError> {
    let header = request
        .headers()
        .get("authorization")
        .map_err(|_| ApiError::unauthorized())?
        .ok_or_else(ApiError::unauthorized)?;
    let (scheme, token) = header.split_once(' ').ok_or_else(ApiError::unauthorized)?;
    if !scheme.eq_ignore_ascii_case("Bearer") || !valid_bearer_token(token) {
        return Err(ApiError::unauthorized());
    }
    Ok(token.to_owned())
}

fn valid_bearer_token(token: &str) -> bool {
    let (value, padding) = token.split_once('=').unwrap_or((token, ""));
    (32..=256).contains(&token.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte))
        && padding.bytes().all(|byte| byte == b'=')
}

async fn get_snapshot(
    request: &Request,
    database: D1Database,
    key: String,
    repository_key: &str,
    scope_id: &str,
    request_id: &str,
) -> Result<Response, ApiError> {
    let (state, _) = load_model(&database, &key).await?.ok_or_else(|| {
        ApiError::new(
            404,
            "history_not_found",
            "Not Found",
            "No currently usable history exists for this scope.",
        )
    })?;
    ensure_path_scope(&state, repository_key, scope_id)?;
    let now_ms = now_ms();
    let table = project_predictions(&state, now_ms).map_err(|_| ApiError::corrupt())?;
    if table.rows.is_empty() {
        return Err(ApiError::new(
            404,
            "history_not_found",
            "Not Found",
            "No currently usable history exists for this scope.",
        ));
    }
    let body = canonical_bytes(&table).map_err(|_| ApiError::corrupt())?;
    let etag = format!("\"sha256:{}\"", sha256_hex(&body));
    let headers = request.headers().get("if-none-match").map_err(|_| {
        ApiError::new(
            400,
            "invalid_request",
            "Bad Request",
            "If-None-Match is invalid.",
        )
    })?;
    if headers
        .as_deref()
        .is_some_and(|value| matches_etag(value, &etag))
    {
        return response(
            304,
            Vec::new(),
            request_id,
            "private, no-cache",
            Some(&etag),
            "application/json",
        );
    }
    response(
        200,
        body,
        request_id,
        "private, no-cache",
        Some(&etag),
        "application/json",
    )
}

async fn merge_observations(
    request: &mut Request,
    database: D1Database,
    key: String,
    repository_key: &str,
    scope_id: &str,
    request_id: &str,
) -> Result<Response, ApiError> {
    validate_request_media(request)?;
    let body = read_bounded_body(request).await?;
    let value = json::parse_without_duplicate_keys(&body).map_err(|_| {
        ApiError::new(
            400,
            "invalid_request",
            "Bad Request",
            "Request body must be valid UTF-8 JSON with unique keys.",
        )
    })?;
    let batch: ObservationBatch = serde_json::from_value(value).map_err(|_| {
        ApiError::new(
            422,
            "invalid_history",
            "Unprocessable Content",
            "Observation batch schema is invalid.",
        )
    })?;
    if batch.scope.repository_key != repository_key
        || batch.scope.id().map_err(|_| {
            ApiError::new(
                422,
                "invalid_history",
                "Unprocessable Content",
                "Observation scope is invalid.",
            )
        })? != scope_id
    {
        return Err(ApiError::new(
            403,
            "scope_mismatch",
            "Forbidden",
            "Observation scope does not match the authorized URL.",
        ));
    }
    let current_time = now_ms();
    batch
        .validate(current_time)
        .map_err(|error| map_batch_validation_error(&error))?;
    let body_digest = batch.body_digest().map_err(|_| {
        ApiError::new(
            422,
            "invalid_history",
            "Unprocessable Content",
            "Observation batch digest is invalid.",
        )
    })?;
    let idempotency_key = request
        .headers()
        .get("idempotency-key")
        .map_err(|_| {
            ApiError::new(
                400,
                "invalid_request",
                "Bad Request",
                "Idempotency-Key is invalid.",
            )
        })?
        .ok_or_else(|| {
            ApiError::new(
                400,
                "idempotency_key_mismatch",
                "Bad Request",
                "Idempotency-Key is required.",
            )
        })?;
    if idempotency_key != body_digest {
        return Err(ApiError::new(
            409,
            "idempotency_key_mismatch",
            "Conflict",
            "Idempotency-Key must equal the canonical batch digest.",
        ));
    }

    for attempt in 0..CAS_ATTEMPTS {
        let stored = load_model(&database, &key).await?;
        let (previous, expected_digest) = match stored {
            Some((state, digest)) => {
                ensure_path_scope(&state, repository_key, scope_id)?;
                if let Some(receipt) = state
                    .receipts
                    .iter()
                    .find(|receipt| receipt.0 == batch.batch_id)
                {
                    if receipt.1 == body_digest {
                        let result = merge_result(&state, &batch, false, 0, true);
                        return json_response(200, &result, request_id, "no-store");
                    }
                    return Err(ApiError::new(
                        409,
                        "batch_conflict",
                        "Conflict",
                        "Batch identity already exists with different content.",
                    ));
                }
                (Some(state), Some(digest))
            }
            None => (None, None),
        };
        let previously_existed = previous.is_some();
        let (updated, outcome) =
            apply_batch(previous, &batch, current_time).map_err(|error| map_apply_error(&error))?;
        let ApplyOutcome::Applied { pruned_buckets } = outcome else {
            let result = merge_result(&updated, &batch, false, 0, previously_existed);
            return json_response(200, &result, request_id, "no-store");
        };
        let model_json =
            String::from_utf8(canonical_bytes(&updated).map_err(|_| ApiError::corrupt())?)
                .map_err(|_| ApiError::corrupt())?;
        if model_json.len() > MAX_MODEL_BYTES {
            return Err(ApiError::new(
                409,
                "model_capacity_exceeded",
                "Conflict",
                "History model exceeds the 1.9 MB D1 row limit.",
            ));
        }
        let checksum = sha256_hex(model_json.as_bytes());
        if store_model(
            &database,
            &key,
            expected_digest.as_deref(),
            &model_json,
            &checksum,
            updated.updated_at_ms,
        )
        .await?
        {
            let result = merge_result(&updated, &batch, true, pruned_buckets, previously_existed);
            return json_response(200, &result, request_id, "no-store");
        }
        if attempt + 1 < CAS_ATTEMPTS {
            let ceiling_ms = (10_u64 << attempt.min(4)).min(200);
            let delay_ms = (js_sys::Math::random() * (ceiling_ms as f64 + 1.0)) as u64;
            Delay::from(Duration::from_millis(delay_ms)).await;
        }
    }
    Err(ApiError::new(
        503,
        "cas_retries_exhausted",
        "Service Unavailable",
        "Concurrent history updates exceeded the retry limit.",
    ))
}

fn merge_result(
    state: &ModelState,
    batch: &ObservationBatch,
    applied: bool,
    pruned_bucket_count: u64,
    previously_existed: bool,
) -> MergeResult {
    MergeResult {
        scope_id: state.scope.id().unwrap_or_default(),
        model_version: state.version,
        outcome: if !applied {
            "unchanged"
        } else if previously_existed {
            "updated"
        } else {
            "created"
        },
        applied,
        received_aggregate_count: batch.aggregates.len(),
        retained_key_count: state.entries.len(),
        pruned_bucket_count,
        receipt_count: state.receipts.len(),
        model_updated_at_ms: state.updated_at_ms,
    }
}

fn map_apply_error(error: &str) -> ApiError {
    if error.contains("different body digest") {
        ApiError::new(
            409,
            "batch_conflict",
            "Conflict",
            "Batch identity already exists with different content.",
        )
    } else if error.contains("receipt capacity") {
        ApiError::new(
            409,
            "receipt_capacity_exceeded",
            "Conflict",
            "Retained idempotency receipts are at capacity.",
        )
    } else if error.contains("capacity")
        || error.contains("exceeds 50000")
        || error.contains("16 MiB")
    {
        ApiError::new(
            409,
            "model_capacity_exceeded",
            "Conflict",
            "History model capacity would be exceeded.",
        )
    } else {
        ApiError::new(
            422,
            "invalid_history",
            "Unprocessable Content",
            "Observation batch could not be applied.",
        )
    }
}

fn map_batch_validation_error(error: &str) -> ApiError {
    if error.contains("expired")
        || error.contains("future")
        || error.contains("acceptance watermark")
    {
        ApiError::new(
            422,
            "batch_expired",
            "Unprocessable Content",
            "Observation batch is expired or too far in the future.",
        )
    } else {
        ApiError::new(
            422,
            "invalid_history",
            "Unprocessable Content",
            "Observation batch validation failed.",
        )
    }
}

fn validate_request_media(request: &Request) -> Result<(), ApiError> {
    let content_type = request
        .headers()
        .get("content-type")
        .map_err(|_| {
            ApiError::new(
                400,
                "invalid_request",
                "Bad Request",
                "Content-Type is invalid.",
            )
        })?
        .ok_or_else(|| {
            ApiError::new(
                415,
                "unsupported_media_type",
                "Unsupported Media Type",
                "Only uncompressed application/json UTF-8 is supported.",
            )
        })?;
    if !is_json_utf8(&content_type) {
        return Err(ApiError::new(
            415,
            "unsupported_media_type",
            "Unsupported Media Type",
            "Only uncompressed application/json UTF-8 is supported.",
        ));
    }
    if request
        .headers()
        .get("content-encoding")
        .map_err(|_| {
            ApiError::new(
                400,
                "invalid_request",
                "Bad Request",
                "Content-Encoding is invalid.",
            )
        })?
        .is_some_and(|encoding| !encoding.eq_ignore_ascii_case("identity"))
    {
        return Err(ApiError::new(
            415,
            "unsupported_media_type",
            "Unsupported Media Type",
            "Only uncompressed application/json UTF-8 is supported.",
        ));
    }
    if let Some(length) = request.headers().get("content-length").map_err(|_| {
        ApiError::new(
            400,
            "invalid_request",
            "Bad Request",
            "Content-Length is invalid.",
        )
    })? {
        let length = length.parse::<u64>().map_err(|_| {
            ApiError::new(
                400,
                "invalid_request",
                "Bad Request",
                "Content-Length is invalid.",
            )
        })?;
        if length > MAX_REQUEST_BYTES as u64 {
            return Err(ApiError::new(
                413,
                "payload_too_large",
                "Content Too Large",
                "Request exceeds 16 MiB.",
            ));
        }
    }
    Ok(())
}

fn is_json_utf8(content_type: &str) -> bool {
    let mut parts = content_type.split(';');
    if !parts
        .next()
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    {
        return false;
    }
    match parts.next() {
        None => true,
        Some(parameter) => {
            let Some((name, value)) = parameter.trim().split_once('=') else {
                return false;
            };
            parts.next().is_none()
                && name.trim().eq_ignore_ascii_case("charset")
                && value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
        }
    }
}

async fn read_bounded_body(request: &mut Request) -> Result<Vec<u8>, ApiError> {
    let mut stream = request.stream().map_err(|_| {
        ApiError::new(
            400,
            "invalid_request",
            "Bad Request",
            "Request body is unavailable.",
        )
    })?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            ApiError::new(
                400,
                "invalid_request",
                "Bad Request",
                "Request body could not be read.",
            )
        })?;
        if bytes.len().saturating_add(chunk.len()) > MAX_REQUEST_BYTES {
            return Err(ApiError::new(
                413,
                "payload_too_large",
                "Content Too Large",
                "Request exceeds 16 MiB.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn history_database(env: &Env) -> Result<D1Database, ApiError> {
    env.d1("PREDICTION_STATE").map_err(|_| ApiError::storage())
}

#[derive(Deserialize)]
struct StoredModel {
    model_json: String,
    model_sha256: String,
}

async fn load_model(
    database: &D1Database,
    key: &str,
) -> Result<Option<(ModelState, String)>, ApiError> {
    let key_value = D1Type::Text(key);
    let stored = database
        .prepare("SELECT model_json, model_sha256 FROM prediction_state WHERE storage_key = ?1")
        .bind_refs([&key_value])
        .map_err(|_| ApiError::storage())?
        .first::<StoredModel>(None)
        .await
        .map_err(|_| ApiError::storage())?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let bytes = stored.model_json.as_bytes();
    if bytes.len() > MAX_MODEL_BYTES || stored.model_sha256 != sha256_hex(bytes) {
        return Err(ApiError::corrupt());
    }
    let state = decode_model(bytes)?;
    Ok(Some((state, stored.model_sha256)))
}

async fn store_model(
    database: &D1Database,
    key: &str,
    expected_digest: Option<&str>,
    model_json: &str,
    model_sha256: &str,
    updated_at_ms: u64,
) -> Result<bool, ApiError> {
    let model_json = D1Type::Text(model_json);
    let model_sha256 = D1Type::Text(model_sha256);
    let updated_at_ms = D1Type::Real(updated_at_ms as f64);
    let storage_key = D1Type::Text(key);
    let statement = if let Some(expected_digest) = expected_digest {
        let expected_digest = D1Type::Text(expected_digest);
        database
            .prepare(
                "UPDATE prediction_state SET model_json = ?1, model_sha256 = ?2, updated_at_ms = ?3 WHERE storage_key = ?4 AND model_sha256 = ?5",
            )
            .bind_refs([
                &model_json,
                &model_sha256,
                &updated_at_ms,
                &storage_key,
                &expected_digest,
            ])
    } else {
        database
            .prepare(
                "INSERT OR IGNORE INTO prediction_state (storage_key, model_json, model_sha256, updated_at_ms) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind_refs([&storage_key, &model_json, &model_sha256, &updated_at_ms])
    }
    .map_err(|_| ApiError::storage())?;
    let result = statement.run().await.map_err(|_| ApiError::storage())?;
    let changes = result
        .meta()
        .map_err(|_| ApiError::storage())?
        .and_then(|meta| meta.changes)
        .unwrap_or_default();
    Ok(changes == 1)
}

fn model_key(repository_key: &str, scope_id: &str) -> String {
    format!("nanoom/prediction-state/v3/repositories/{repository_key}/scopes/{scope_id}/model.json")
}

fn ensure_path_scope(
    state: &ModelState,
    repository_key: &str,
    scope_id: &str,
) -> Result<(), ApiError> {
    if state.scope.repository_key != repository_key
        || state.scope.id().map_err(|_| ApiError::corrupt())? != scope_id
    {
        return Err(ApiError::corrupt());
    }
    Ok(())
}

fn decode_model(bytes: &[u8]) -> Result<ModelState, ApiError> {
    let value = json::parse_without_duplicate_keys(bytes).map_err(|_| ApiError::corrupt())?;
    let state: ModelState = serde_json::from_value(value).map_err(|_| ApiError::corrupt())?;
    validate_model(&state).map_err(|_| ApiError::corrupt())?;
    Ok(state)
}

fn now_ms() -> u64 {
    Date::now().as_millis()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(bytes)
}

fn matches_etag(header: &str, current: &str) -> bool {
    header.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate == current || candidate.strip_prefix("W/") == Some(current)
    })
}

fn json_response<T: Serialize>(
    status: u16,
    value: &T,
    request_id: &str,
    cache_control: &str,
) -> Result<Response, ApiError> {
    let body = serde_json::to_vec(value).map_err(|_| {
        ApiError::new(
            500,
            "internal_error",
            "Internal Server Error",
            "Response could not be serialized.",
        )
    })?;
    response(
        status,
        body,
        request_id,
        cache_control,
        None,
        "application/json",
    )
}

fn problem_response(error: ApiError, request_id: &str) -> worker::Result<Response> {
    let body = serde_json::to_vec(&serde_json::json!({
        "type": "about:blank",
        "title": error.title,
        "status": error.status,
        "code": error.code,
        "detail": error.detail,
        "requestId": request_id,
    }))
    .unwrap_or_else(|_| b"{}".to_vec());
    let headers = Headers::new();
    headers.set("content-type", "application/problem+json")?;
    headers.set("cache-control", "no-store")?;
    headers.set("x-request-id", request_id)?;
    if error.authenticate {
        headers.set("www-authenticate", "Bearer")?;
    }
    if let Some(allow) = error.allow {
        headers.set("allow", allow)?;
    }
    if error.status == 429 || error.status == 503 {
        headers.set("retry-after", "1")?;
    }
    Response::from_bytes(body)
        .map(|response| response.with_status(error.status).with_headers(headers))
}

fn response(
    status: u16,
    body: Vec<u8>,
    request_id: &str,
    cache_control: &str,
    etag: Option<&str>,
    content_type: &str,
) -> Result<Response, ApiError> {
    let headers = Headers::new();
    headers.set("content-type", content_type).map_err(|_| {
        ApiError::new(
            500,
            "internal_error",
            "Internal Server Error",
            "Response headers could not be set.",
        )
    })?;
    headers.set("cache-control", cache_control).map_err(|_| {
        ApiError::new(
            500,
            "internal_error",
            "Internal Server Error",
            "Response headers could not be set.",
        )
    })?;
    headers.set("x-request-id", request_id).map_err(|_| {
        ApiError::new(
            500,
            "internal_error",
            "Internal Server Error",
            "Response headers could not be set.",
        )
    })?;
    if let Some(etag) = etag {
        headers.set("etag", etag).map_err(|_| {
            ApiError::new(
                500,
                "internal_error",
                "Internal Server Error",
                "Response headers could not be set.",
            )
        })?;
    }
    if status == 503 {
        headers.set("retry-after", "1").map_err(|_| {
            ApiError::new(
                500,
                "internal_error",
                "Internal Server Error",
                "Response headers could not be set.",
            )
        })?;
    }
    let response = if status == 304 {
        Response::empty()
    } else {
        Response::from_bytes(body)
    };
    response
        .map(|response| response.with_status(status).with_headers(headers))
        .map_err(|_| {
            ApiError::new(
                500,
                "internal_error",
                "Internal Server Error",
                "Response could not be created.",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{is_json_utf8, matches_etag, parse_history_path, valid_bearer_token, Operation};

    #[test]
    fn accepts_only_exact_history_routes() {
        let scope_id = "a".repeat(64);
        assert!(matches!(
            parse_history_path(&format!(
                "/v1/repositories/github-123/scopes/{scope_id}/snapshot"
            )),
            Some(("github-123", _, Operation::Snapshot))
        ));
        assert!(parse_history_path("/v1/repositories/*/scopes/all/snapshot").is_none());
        assert!(parse_history_path(&format!(
            "/v1/repositories/github-123/scopes/{scope_id}/snapshot/extra"
        ))
        .is_none());
    }

    #[test]
    fn validates_bearer_tokens_and_json_charset() {
        assert!(valid_bearer_token(&"a".repeat(32)));
        assert!(valid_bearer_token(&format!("{}==", "a".repeat(32))));
        assert!(!valid_bearer_token("short"));
        assert!(!valid_bearer_token(&format!("{}a=b", "a".repeat(32))));
        assert!(is_json_utf8("Application/JSON; charset=\"utf-8\""));
        assert!(!is_json_utf8("application/json; charset=iso-8859-1"));
        assert!(!is_json_utf8("application/json; charset=utf-8; boundary=x"));
    }

    #[test]
    fn supports_if_none_match_weak_and_wildcard_comparison() {
        let etag = "\"sha256:abc\"";
        assert!(matches_etag(etag, etag));
        assert!(matches_etag("W/\"sha256:abc\"", etag));
        assert!(matches_etag("\"other\", *", etag));
        assert!(!matches_etag("\"other\"", etag));
    }
}
