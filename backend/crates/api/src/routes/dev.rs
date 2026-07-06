use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

pub async fn jwks(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let issuer = state
        .dev_issuer
        .as_ref()
        .ok_or_else(|| ApiError::internal("dev issuer not enabled"))?;
    Ok(Json(issuer.jwks_json.clone()))
}

#[derive(Deserialize)]
pub struct TokenRequest {
    user_id: String,
    org_id: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    token: String,
}

pub async fn token(
    State(state): State<AppState>,
    Json(body): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    let issuer = state
        .dev_issuer
        .as_ref()
        .ok_or_else(|| ApiError::internal("dev issuer not enabled"))?;
    let token = issuer
        .issue_token(
            &state.config.auth_issuer,
            &state.config.auth_org_claim,
            &body.user_id,
            &body.org_id,
        )
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(TokenResponse { token }))
}
