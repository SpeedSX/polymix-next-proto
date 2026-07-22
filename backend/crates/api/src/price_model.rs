//! In-memory `PriceModel` snapshot cache (A2a-4).
//!
//! The pricing endpoints (staff `/api/estimate`, later the portal) price
//! against an `Arc<PriceModel>` built from the catalog tables, never touching
//! the DB per request. Snapshots are cached per `(tenant_db, pricelist_version)`
//! so a rate edit — which bumps the version — is picked up on the next request
//! without a restart.
//!
//! v1 invalidation is a cheap point-read of `meta:pricing.version` per lookup
//! (`docs/pricing-admin-plan.md` A2a-4). The SurrealDB live-query rebuild from
//! `instant-quote.md` is a later optimization behind this same interface.

use std::sync::Arc;

use domain::PricingRepo;
use domain::error::DomainError;
use moka::future::Cache;
use quote_engine::PriceModel;

/// Bounds the reload retries when the catalog is being mutated concurrently, so
/// a pathological write storm can't spin forever. Well above any real edit rate.
const MAX_LOAD_ATTEMPTS: usize = 5;

pub struct PriceModelCache {
    cache: Cache<(String, i64), Arc<PriceModel>>,
}

impl PriceModelCache {
    pub fn new() -> Self {
        Self {
            cache: Cache::new(256),
        }
    }

    /// The tenant's current price-model snapshot, rebuilt only when the
    /// pricelist version has moved since it was last cached.
    ///
    /// On a cache miss the catalog is loaded and its version re-read; if the
    /// two disagree the load raced a mutation and is retried, so a torn
    /// snapshot is never cached under a stable version.
    pub async fn get(
        &self,
        repo: &dyn PricingRepo,
        tenant_db: &str,
    ) -> Result<Arc<PriceModel>, DomainError> {
        for _ in 0..MAX_LOAD_ATTEMPTS {
            let version = repo.get_version().await?;
            if let Some(model) = self.cache.get(&(tenant_db.to_string(), version)).await {
                return Ok(model);
            }

            let dataset = repo.load_dataset().await?;
            let loaded_version = dataset.pricelist_version;
            let after = repo.get_version().await?;
            if loaded_version != after {
                continue;
            }

            let model = Arc::new(PriceModel::from_dataset(dataset));
            self.cache
                .insert((tenant_db.to_string(), loaded_version), Arc::clone(&model))
                .await;
            return Ok(model);
        }
        Err(DomainError::Store(
            "pricing catalog changed faster than the snapshot could be built".to_string(),
        ))
    }
}

impl Default for PriceModelCache {
    fn default() -> Self {
        Self::new()
    }
}
