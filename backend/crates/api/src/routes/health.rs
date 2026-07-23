use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::{Value, json};

use crate::state::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

const READY_QUERY_TIMEOUT: Duration = Duration::from_secs(1);

/// Unlike `health`, this depends on SurrealDB — point the orchestrator's
/// readiness (not liveness) probe here. Never wire the DB into liveness: a
/// DB outage would otherwise turn into an API restart loop instead of the
/// orchestrator simply pausing traffic until `ready` recovers.
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match tokio::time::timeout(READY_QUERY_TIMEOUT, state.backend.ping()).await {
        Ok(Ok(_)) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
        Ok(Err(err)) => {
            tracing::error!(error = %err, "readiness check query failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
        }
        Err(_) => {
            tracing::error!("readiness check timed out");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
        }
    }
}
