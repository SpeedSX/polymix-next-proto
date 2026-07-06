use std::env;

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
    pub auth_dev_mode: bool,
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
            surrealdb_url: env_or("SURREALDB_URL", "ws://localhost:8000"),
            surrealdb_user: env_or("SURREALDB_USER", "root"),
            surrealdb_pass: env_or("SURREALDB_PASS", "root"),
            surrealdb_ns: env_or("SURREALDB_NS", "polymix"),
            auth_issuer,
            auth_jwks_url,
            auth_org_claim: env_or("AUTH_ORG_CLAIM", "org_id"),
            auth_dev_mode,
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
