use axum::Extension;
use axum::Json;
use domain::{AuthContext, Tenant};
use serde::Serialize;

#[derive(Serialize)]
pub struct TenantSummary {
    name: String,
    default_language: String,
    default_currency: String,
}

#[derive(Serialize)]
pub struct MeResponse {
    user_id: String,
    org_id: String,
    tenant: TenantSummary,
}

pub async fn me(
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
) -> Json<MeResponse> {
    Json(MeResponse {
        user_id: auth.user_id,
        org_id: auth.org_id,
        tenant: TenantSummary {
            name: tenant.name,
            default_language: tenant.default_language,
            default_currency: tenant.default_currency,
        },
    })
}
