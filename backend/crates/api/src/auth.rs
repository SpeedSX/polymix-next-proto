use axum::body::Body;
use axum::extract::State;
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, Request};
use axum::middleware::Next;
use axum::response::Response;
use domain::AuthContext;
use jsonwebtoken::{Algorithm, Validation};

use crate::config::AppConfig;
use crate::error::ApiError;
use crate::jwks::JwksCache;
use crate::state::AppState;

fn bearer_token(headers: &HeaderMap) -> Result<&str, ApiError> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing authorization header"))?;
    header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("invalid authorization header"))
}

/// Verifies the token and extracts the caller's identity. Split out from
/// `authenticate_token` so it can be unit-tested without a
/// `TenantProvisioner` (which requires a live database connection to
/// construct).
async fn validate_token(
    token: &str,
    jwks: &JwksCache,
    config: &AppConfig,
) -> Result<(String, String, String), ApiError> {
    let header_data =
        jsonwebtoken::decode_header(token).map_err(|_| ApiError::unauthorized("invalid token"))?;
    let kid = header_data
        .kid
        .ok_or_else(|| ApiError::unauthorized("token missing kid"))?;
    let key = jwks.get_key(&kid).await?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(std::slice::from_ref(&config.auth_issuer));
    match &config.auth_audience {
        Some(audience) => validation.set_audience(std::slice::from_ref(audience)),
        // Dev tokens and Clerk's default session tokens carry no `aud` claim, so this is a
        // no-op for them; it only matters once a Clerk JWT template adds one.
        None => validation.validate_aud = false,
    }
    let token_data = jsonwebtoken::decode::<serde_json::Value>(token, &key, &validation)
        .map_err(|_| ApiError::unauthorized("token validation failed"))?;

    let claims = token_data.claims;
    let user_id = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("token missing sub"))?
        .to_string();
    let org_id = claims
        .get(&config.auth_org_claim)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::forbidden("no active organization"))?
        .to_string();
    // Clerk's default session token has no org display name; a custom "org_name"
    // claim (added via the Clerk dashboard's session token template) supplies it.
    // Falls back to org_id for the dev issuer and any Clerk instance without it.
    let display_name = claims
        .get("org_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&org_id)
        .to_string();

    Ok((user_id, org_id, display_name))
}

/// The header-independent auth core: JWT validation, org-claim check, and
/// tenant resolution including auto-provisioning. `require_auth` reads the
/// bearer header and calls this; the WS handler calls it directly with the
/// `?token=` query parameter (browsers can't set headers on WS upgrades).
pub async fn authenticate_token(
    state: &AppState,
    token: &str,
) -> Result<(AuthContext, domain::Tenant), ApiError> {
    let (user_id, org_id, display_name) = validate_token(token, &state.jwks, &state.config).await?;

    let tenant = state
        .provisioner
        .ensure_tenant(&org_id, &display_name)
        .await?;

    let auth_ctx = AuthContext {
        user_id,
        org_id,
        tenant_db: tenant.db_name.clone(),
    };
    Ok((auth_ctx, tenant))
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let token = bearer_token(req.headers())?;
    let (auth_ctx, tenant) = authenticate_token(&state, token).await?;

    req.extensions_mut().insert(auth_ctx);
    req.extensions_mut().insert(tenant);

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;
    use axum::routing::get;
    use axum::{Json, Router};

    use super::*;
    use crate::dev_issuer::DevIssuer;

    fn test_config(issuer: &str, jwks_url: &str) -> AppConfig {
        AppConfig {
            port: 0,
            surrealdb_url: String::new(),
            surrealdb_user: String::new(),
            surrealdb_pass: String::new(),
            surrealdb_ns: String::new(),
            auth_issuer: issuer.to_string(),
            auth_jwks_url: jwks_url.to_string(),
            auth_org_claim: "org_id".to_string(),
            auth_audience: None,
            auth_dev_mode: true,
            cors_allowed_origins: None,
        }
    }

    async fn serve_jwks(jwks_json: serde_json::Value) -> String {
        let router = Router::new().route("/jwks.json", get(move || async move { Json(jwks_json) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}/jwks.json")
    }

    #[tokio::test]
    async fn missing_header_is_unauthorized() {
        let err = bearer_token(&HeaderMap::new()).unwrap_err();

        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "missing authorization header");
    }

    #[tokio::test]
    async fn non_bearer_header_is_unauthorized() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );

        let err = bearer_token(&headers).unwrap_err();

        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "invalid authorization header");
    }

    #[tokio::test]
    async fn malformed_token_is_unauthorized() {
        let jwks = JwksCache::new("http://unused.invalid/jwks.json".to_string());
        let config = test_config("issuer", "http://unused.invalid/jwks.json");

        let err = validate_token("not-a-jwt", &jwks, &config)
            .await
            .unwrap_err();

        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "invalid token");
    }

    /// Header extraction and token validation fail independently: a
    /// well-formed Bearer header still yields the token even when the token
    /// itself is garbage — the failure then comes from validation, not
    /// header parsing.
    #[tokio::test]
    async fn header_extraction_is_independent_of_token_validity() {
        let jwks = JwksCache::new("http://unused.invalid/jwks.json".to_string());
        let config = test_config("issuer", "http://unused.invalid/jwks.json");
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer not-a-jwt"));

        let token = bearer_token(&headers).unwrap();
        assert_eq!(token, "not-a-jwt");

        let err = validate_token(token, &jwks, &config).await.unwrap_err();
        assert_eq!(err.message, "invalid token");
    }

    #[tokio::test]
    async fn missing_org_claim_is_forbidden() {
        let issuer = DevIssuer::generate().unwrap();
        let jwks_url = serve_jwks(issuer.jwks_json.clone()).await;
        let jwks = JwksCache::new(jwks_url.clone());
        let config = test_config("test-issuer", &jwks_url);
        let token = issuer
            .issue_token_with_claims(serde_json::json!({
                "iss": "test-issuer",
                "sub": "user-1",
                "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
            }))
            .unwrap();

        let err = validate_token(&token, &jwks, &config).await.unwrap_err();

        assert_eq!(err.status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(err.message, "no active organization");
    }

    #[tokio::test]
    async fn valid_token_authenticates() {
        let issuer = DevIssuer::generate().unwrap();
        let jwks_url = serve_jwks(issuer.jwks_json.clone()).await;
        let jwks = JwksCache::new(jwks_url.clone());
        let config = test_config("test-issuer", &jwks_url);
        let token = issuer
            .issue_token("test-issuer", "org_id", "user-1", "org-1")
            .unwrap();

        let (user_id, org_id, display_name) = validate_token(&token, &jwks, &config).await.unwrap();

        assert_eq!(user_id, "user-1");
        assert_eq!(org_id, "org-1");
        assert_eq!(display_name, "org-1");
    }

    #[tokio::test]
    async fn org_name_claim_is_used_as_display_name() {
        let issuer = DevIssuer::generate().unwrap();
        let jwks_url = serve_jwks(issuer.jwks_json.clone()).await;
        let jwks = JwksCache::new(jwks_url.clone());
        let config = test_config("test-issuer", &jwks_url);
        let token = issuer
            .issue_token_with_claims(serde_json::json!({
                "iss": "test-issuer",
                "sub": "user-1",
                "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
                "org_id": "org-1",
                "org_name": "Adamant Print GmbH",
            }))
            .unwrap();

        let (_, _, display_name) = validate_token(&token, &jwks, &config).await.unwrap();

        assert_eq!(display_name, "Adamant Print GmbH");
    }
}
