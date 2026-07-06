use std::collections::HashMap;
use std::time::{Duration, Instant};

use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use tokio::sync::RwLock;

const REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, thiserror::Error)]
pub enum JwksError {
    /// The JWKS endpoint itself is unreachable, slow, or returned garbage —
    /// an upstream dependency failure, not the caller's fault.
    #[error("failed to fetch JWKS: {0}")]
    FetchFailed(String),
    /// The JWKS was fetched successfully but doesn't contain this `kid`.
    #[error("unknown signing key: {0}")]
    UnknownKid(String),
}

#[derive(Deserialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
}

struct CacheState {
    keys: HashMap<String, DecodingKey>,
    last_refresh: Option<Instant>,
}

/// Caches JWKS keys in memory, refreshing on an unknown `kid` but never more
/// often than once per REFRESH_INTERVAL.
pub struct JwksCache {
    url: String,
    client: reqwest::Client,
    state: RwLock<CacheState>,
}

impl JwksCache {
    pub fn new(url: String) -> Self {
        Self {
            url,
            // Without a timeout, a hung JWKS endpoint blocks the refresh
            // indefinitely — and every other request waits on it too, since
            // refresh holds the cache's write lock for its duration.
            client: reqwest::Client::builder()
                .timeout(FETCH_TIMEOUT)
                .build()
                .expect("reqwest client with a timeout should always build"),
            state: RwLock::new(CacheState {
                keys: HashMap::new(),
                last_refresh: None,
            }),
        }
    }

    pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, JwksError> {
        if let Some(key) = self.state.read().await.keys.get(kid) {
            return Ok(key.clone());
        }

        self.refresh_if_due().await?;

        self.state
            .read()
            .await
            .keys
            .get(kid)
            .cloned()
            .ok_or_else(|| JwksError::UnknownKid(kid.to_string()))
    }

    async fn refresh_if_due(&self) -> Result<(), JwksError> {
        let mut state = self.state.write().await;
        if let Some(last) = state.last_refresh
            && last.elapsed() < REFRESH_INTERVAL
        {
            return Ok(());
        }

        let response: JwksResponse = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| JwksError::FetchFailed(e.to_string()))?
            .json()
            .await
            .map_err(|e| JwksError::FetchFailed(e.to_string()))?;
        let mut keys = HashMap::with_capacity(response.keys.len());
        for jwk in response.keys {
            let key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
                .map_err(|e| JwksError::FetchFailed(e.to_string()))?;
            keys.insert(jwk.kid, key);
        }
        state.keys = keys;
        state.last_refresh = Some(Instant::now());
        Ok(())
    }
}
