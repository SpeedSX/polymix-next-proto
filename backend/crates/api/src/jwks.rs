use std::collections::HashMap;
use std::time::{Duration, Instant};

use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

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
    // Serializes the HTTP fetch itself (held across the .await), so
    // concurrent unknown-kid lookups share one in-flight refresh instead of
    // each firing their own request.
    fetch_lock: Mutex<()>,
}

impl JwksCache {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::builder()
                .timeout(FETCH_TIMEOUT)
                .build()
                .expect("reqwest client with a timeout should always build"),
            state: RwLock::new(CacheState {
                keys: HashMap::new(),
                last_refresh: None,
            }),
            fetch_lock: Mutex::new(()),
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
        if self.is_fresh().await {
            return Ok(());
        }

        // Only one task fetches at a time; the rest wait here instead of
        // each firing their own request, then find the cache already fresh.
        let _fetch_guard = self.fetch_lock.lock().await;
        if self.is_fresh().await {
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

        let mut state = self.state.write().await;
        state.keys = keys;
        state.last_refresh = Some(Instant::now());
        Ok(())
    }

    async fn is_fresh(&self) -> bool {
        self.state
            .read()
            .await
            .last_refresh
            .is_some_and(|last| last.elapsed() < REFRESH_INTERVAL)
    }
}
