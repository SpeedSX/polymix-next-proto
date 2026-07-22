use std::env;

use axum::http::HeaderValue;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub surrealdb_url: String,
    pub surrealdb_user: String,
    pub surrealdb_pass: String,
    pub surrealdb_ns: String,
    pub auth_issuer: String,
    pub auth_jwks_url: String,
    pub auth_org_claim: String,
    pub auth_audience: Option<String>,
    pub auth_dev_mode: bool,
    /// `None` means permissive (dev mode only) — see `cors_allowed_origins_from_env`.
    pub cors_allowed_origins: Option<Vec<String>>,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let port: u16 = env_or("PORT", "8080").parse()?;
        let auth_dev_mode = env_or("AUTH_DEV_MODE", "false").parse()?;

        let (default_issuer, default_jwks_url) = if auth_dev_mode {
            (
                format!("http://localhost:{port}"),
                format!("http://localhost:{port}/dev/jwks.json"),
            )
        } else {
            (String::new(), String::new())
        };

        let auth_issuer = match env::var("AUTH_ISSUER") {
            Ok(v) => v,
            Err(_) if auth_dev_mode => default_issuer,
            Err(_) => anyhow::bail!("AUTH_ISSUER is required"),
        };
        let auth_jwks_url = match env::var("AUTH_JWKS_URL") {
            Ok(v) => v,
            Err(_) if auth_dev_mode => default_jwks_url,
            Err(_) => anyhow::bail!("AUTH_JWKS_URL is required"),
        };

        Ok(Self {
            port,
            surrealdb_url: env_or("SURREALDB_URL", "ws://localhost:8001"),
            surrealdb_user: env_or("SURREALDB_USER", "root"),
            surrealdb_pass: env_or("SURREALDB_PASS", "root"),
            surrealdb_ns: env_or("SURREALDB_NS", "polymix"),
            auth_issuer,
            auth_jwks_url,
            auth_org_claim: env_or("AUTH_ORG_CLAIM", "org_id"),
            auth_audience: env::var("AUTH_AUDIENCE").ok(),
            auth_dev_mode,
            cors_allowed_origins: parse_cors_allowed_origins(
                env::var("CORS_ALLOWED_ORIGINS").ok().as_deref(),
                auth_dev_mode,
            )?,
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Unset + dev mode -> `None` (permissive, unchanged local DX). Unset +
/// non-dev -> startup error: a prod deploy must never silently run
/// permissive CORS. Set -> the comma-separated exact origins, validated as
/// well-formed header values now so a malformed one fails at startup rather
/// than at the first preflight. Takes the already-read env value (rather
/// than reading `CORS_ALLOWED_ORIGINS` itself) so it's a pure function unit
/// tests can drive without mutating process env.
fn parse_cors_allowed_origins(
    raw: Option<&str>,
    auth_dev_mode: bool,
) -> anyhow::Result<Option<Vec<String>>> {
    match raw {
        Some(value) => {
            let origins: Vec<String> = value
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_string)
                .collect();
            for origin in &origins {
                origin.parse::<HeaderValue>().map_err(|_| {
                    anyhow::anyhow!("CORS_ALLOWED_ORIGINS contains an invalid origin: {origin}")
                })?;
            }
            Ok(Some(origins))
        }
        None if auth_dev_mode => Ok(None),
        None => anyhow::bail!(
            "CORS_ALLOWED_ORIGINS is required outside AUTH_DEV_MODE (a prod deploy must never \
             silently run permissive CORS)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_in_dev_mode_is_permissive() {
        let result = parse_cors_allowed_origins(None, true).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn unset_outside_dev_mode_fails_startup() {
        let err = parse_cors_allowed_origins(None, false).unwrap_err();
        assert!(err.to_string().contains("CORS_ALLOWED_ORIGINS"));
    }

    #[test]
    fn set_splits_trims_and_drops_empty_entries() {
        let result =
            parse_cors_allowed_origins(Some(" https://a.example , https://b.example ,"), false)
                .unwrap();
        assert_eq!(
            result,
            Some(vec![
                "https://a.example".to_string(),
                "https://b.example".to_string()
            ])
        );
    }

    #[test]
    fn set_in_dev_mode_still_takes_the_explicit_value() {
        let result = parse_cors_allowed_origins(Some("https://a.example"), true).unwrap();
        assert_eq!(result, Some(vec!["https://a.example".to_string()]));
    }

    #[test]
    fn invalid_origin_fails_startup() {
        let err =
            parse_cors_allowed_origins(Some("not a valid header value \u{0}"), false).unwrap_err();
        assert!(err.to_string().contains("invalid origin"));
    }
}
