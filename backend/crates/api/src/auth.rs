use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::Response;
use domain::AuthContext;
use jsonwebtoken::{Algorithm, Validation};

use crate::error::ApiError;
use crate::state::AppState;

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing authorization header"))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("invalid authorization header"))?;

    let header_data =
        jsonwebtoken::decode_header(token).map_err(|_| ApiError::unauthorized("invalid token"))?;
    let kid = header_data
        .kid
        .ok_or_else(|| ApiError::unauthorized("token missing kid"))?;
    let key = state.jwks.get_key(&kid).await?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(std::slice::from_ref(&state.config.auth_issuer));
    let token_data = jsonwebtoken::decode::<serde_json::Value>(token, &key, &validation)
        .map_err(|_| ApiError::unauthorized("token validation failed"))?;

    let claims = token_data.claims;
    let user_id = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("token missing sub"))?
        .to_string();
    let org_id = claims
        .get(&state.config.auth_org_claim)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::forbidden("no active organization"))?
        .to_string();

    let tenant = state.provisioner.ensure_tenant(&org_id, &org_id).await?;

    let auth_ctx = AuthContext {
        user_id,
        org_id,
        tenant_db: tenant.db_name.clone(),
    };
    req.extensions_mut().insert(auth_ctx);
    req.extensions_mut().insert(tenant);

    Ok(next.run(req).await)
}
