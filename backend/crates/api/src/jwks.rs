use std::collections::HashMap;
use std::time::{Duration, Instant};

use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use tokio::sync::RwLock;

const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

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
            client: reqwest::Client::new(),
            state: RwLock::new(CacheState {
                keys: HashMap::new(),
                last_refresh: None,
            }),
        }
    }

    pub async fn get_key(&self, kid: &str) -> anyhow::Result<DecodingKey> {
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
            .ok_or_else(|| anyhow::anyhow!("unknown signing key: {kid}"))
    }

    async fn refresh_if_due(&self) -> anyhow::Result<()> {
        let mut state = self.state.write().await;
        if let Some(last) = state.last_refresh
            && last.elapsed() < REFRESH_INTERVAL
        {
            return Ok(());
        }

        let response: JwksResponse = self.client.get(&self.url).send().await?.json().await?;
        state.keys.clear();
        for jwk in response.keys {
            let key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)?;
            state.keys.insert(jwk.kid, key);
        }
        state.last_refresh = Some(Instant::now());
        Ok(())
    }
}
