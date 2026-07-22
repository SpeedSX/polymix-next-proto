//! Pricing catalog CRUD (`/api/pricing/*`, A2a-3).
//!
//! One generic handler set serves all five tables, keyed by the `{entity}`
//! path segment (`formats`, `materials`, `machines`, `operations`,
//! `policies`). Bodies and responses are the engine's stored shapes as JSON
//! (see `domain::pricing`); the repo validates structure and bumps the
//! pricelist version on every write.
//!
//! Authorization: these routes sit behind `require_auth` (authenticated +
//! tenant-scoped) only. The `pricing:read` / `pricing:write` gating from
//! `docs/pricing-admin-plan.md` needs the RBAC layer (roadmap B1), which is not
//! yet implemented — see `docs/adr/0014-pricing-routes-gate-on-auth-pending-rbac.md`.

use std::sync::Arc;

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use domain::error::DomainError;
use domain::pricing::PricingEntity;
use domain::{AuthContext, PricingRepo};
use serde_json::{Value, json};

use crate::error::ApiError;
use crate::state::AppState;

async fn repo_for(state: &AppState, auth: &AuthContext) -> Result<Arc<dyn PricingRepo>, ApiError> {
    Ok(state.backend.pricing_repo(&auth.tenant_db).await?)
}

fn parse_entity(segment: &str) -> Result<PricingEntity, ApiError> {
    PricingEntity::from_segment(segment)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "unknown catalog entity"))
}

pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(entity): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let entity = parse_entity(&entity)?;
    let repo = repo_for(&state, &auth).await?;
    let items = repo.list(entity).await?;
    Ok(Json(json!({ "items": items })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(entity): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let entity = parse_entity(&entity)?;
    let repo = repo_for(&state, &auth).await?;
    let created = repo.create(entity, body).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((entity, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let entity = parse_entity(&entity)?;
    let repo = repo_for(&state, &auth).await?;
    let row = repo.get(entity, &id).await?.ok_or(DomainError::NotFound)?;
    Ok(Json(row))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((entity, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let entity = parse_entity(&entity)?;
    let repo = repo_for(&state, &auth).await?;
    let updated = repo.update(entity, &id, body).await?;
    Ok(Json(updated))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((entity, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let entity = parse_entity(&entity)?;
    let repo = repo_for(&state, &auth).await?;
    repo.delete(entity, &id).await?;
    Ok(Json(json!({})))
}

pub async fn version(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Value>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let version = repo.get_version().await?;
    Ok(Json(json!({ "version": version })))
}
