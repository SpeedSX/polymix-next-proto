//! Pricing catalog integration tests (A2a): full CRUD per entity over the HTTP
//! app, tenant isolation, server-side validation, and the version bump on every
//! mutation; plus the DB→`PriceModel` snapshot pricing the §9.1 golden dataset
//! and rebuilding when a rate changes (A2a-4/A2a-5).

mod common;

use std::sync::Arc;

use common::{TestApp, shared_db};
use reqwest::StatusCode;
use serde_json::{Value, json};

use api::price_model::PriceModelCache;
use domain::{PricingEntity, PricingRepo};
use quote_engine::{JobSpec, price_job};
use surreal_store::{DbConfig, Store, SurrealPricingRepo, TenantProvisioner};

/// The engine's own golden fixture, embedded so the seeded-catalog test and the
/// engine test share one source of truth.
const GOLDEN: &str = include_str!("../../quote-engine/fixtures/demo.json");

const CATALOG_TABLES: &[(&str, &str)] = &[
    ("formats", "format"),
    ("materials", "material"),
    ("machines", "machine"),
    ("operations", "operation"),
    ("pricing_policies", "pricing_policy"),
];

/// Percent-encode a record id (`format:a5`) for a URL path segment.
fn encode_id(id: &str) -> String {
    id.replace(':', "%3A")
}

impl TestApp {
    async fn pricing_create(&self, org: &str, entity: &str, body: Value) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!("{}/api/pricing/{entity}", self.base_url))
            .bearer_auth(self.token_for(org))
            .json(&body)
            .send()
            .await
            .expect("create request failed");
        let status = response.status();
        (
            status,
            response.json().await.expect("create response not JSON"),
        )
    }

    async fn pricing_list(&self, org: &str, entity: &str) -> Value {
        self.client
            .get(format!("{}/api/pricing/{entity}", self.base_url))
            .bearer_auth(self.token_for(org))
            .send()
            .await
            .expect("list request failed")
            .json()
            .await
            .expect("list response not JSON")
    }

    async fn pricing_get(&self, org: &str, entity: &str, id: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .get(format!(
                "{}/api/pricing/{entity}/{}",
                self.base_url,
                encode_id(id)
            ))
            .bearer_auth(self.token_for(org))
            .send()
            .await
            .expect("get request failed");
        let status = response.status();
        (
            status,
            response.json().await.expect("get response not JSON"),
        )
    }

    async fn pricing_update(
        &self,
        org: &str,
        entity: &str,
        id: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = self
            .client
            .put(format!(
                "{}/api/pricing/{entity}/{}",
                self.base_url,
                encode_id(id)
            ))
            .bearer_auth(self.token_for(org))
            .json(&body)
            .send()
            .await
            .expect("update request failed");
        let status = response.status();
        (
            status,
            response.json().await.expect("update response not JSON"),
        )
    }

    async fn pricing_delete(&self, org: &str, entity: &str, id: &str) -> StatusCode {
        self.client
            .delete(format!(
                "{}/api/pricing/{entity}/{}",
                self.base_url,
                encode_id(id)
            ))
            .bearer_auth(self.token_for(org))
            .send()
            .await
            .expect("delete request failed")
            .status()
    }

    async fn pricing_version(&self, org: &str) -> i64 {
        let body: Value = self
            .client
            .get(format!("{}/api/pricing/version", self.base_url))
            .bearer_auth(self.token_for(org))
            .send()
            .await
            .expect("version request failed")
            .json()
            .await
            .expect("version response not JSON");
        body["version"].as_i64().expect("version is an integer")
    }
}

#[tokio::test]
#[ignore]
async fn format_crud_round_trips_and_bumps_version() {
    let app = TestApp::spawn().await;
    let org = format!("crud_{}", ulid::Ulid::new());

    let v0 = app.pricing_version(&org).await;

    let (status, created) = app
        .pricing_create(
            &org,
            "formats",
            json!({ "name": "A5", "trim_mm": [148, 210] }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().expect("created id").to_string();
    assert!(id.starts_with("format:"));
    assert_eq!(app.pricing_version(&org).await, v0 + 1);

    let list = app.pricing_list(&org, "formats").await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);

    let (status, fetched) = app.pricing_get(&org, "formats", &id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["name"], "A5");

    let (status, updated) = app
        .pricing_update(
            &org,
            "formats",
            &id,
            json!({ "name": "A5 portrait", "trim_mm": [148, 210] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "A5 portrait");
    assert_eq!(app.pricing_version(&org).await, v0 + 2);

    assert_eq!(
        app.pricing_delete(&org, "formats", &id).await,
        StatusCode::OK
    );
    assert_eq!(app.pricing_version(&org).await, v0 + 3);
    let (status, _) = app.pricing_get(&org, "formats", &id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore]
async fn material_tagged_union_round_trips() {
    let app = TestApp::spawn().await;
    let org = format!("mat_{}", ulid::Ulid::new());

    let (status, created) = app
        .pricing_create(
            &org,
            "materials",
            json!({
                "name": "Offset 80 g", "kind": "paper",
                "pricing": { "basis": "per_sheet", "sheet_size_mm": [320, 450], "price_micro": 40000 },
                "printable": { "grammage_gsm": 80 }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_string();

    let (_, fetched) = app.pricing_get(&org, "materials", &id).await;
    assert_eq!(fetched["pricing"]["basis"], "per_sheet");
    assert_eq!(fetched["pricing"]["price_micro"], 40000);
    assert_eq!(fetched["pricing"]["sheet_size_mm"], json!([320, 450]));
    assert_eq!(fetched["printable"]["grammage_gsm"], 80);

    // A finishing consumable priced per linear cm, no printable block.
    let (status, _) = app
        .pricing_create(
            &org,
            "materials",
            json!({
                "name": "Spiral black", "kind": "wire",
                "pricing": { "basis": "per_cm", "price_micro": 5000 },
                "attrs": { "colour": "black" }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
#[ignore]
async fn validation_rejects_bad_documents() {
    let app = TestApp::spawn().await;
    let org = format!("val_{}", ulid::Ulid::new());

    // Landscape format (width > height) is not portrait.
    let (status, body) = app
        .pricing_create(
            &org,
            "formats",
            json!({ "name": "Bad", "trim_mm": [210, 99] }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(
        body["error"]["details"]["trim_mm"]["code"],
        "portrait_required"
    );

    // A digital machine may not carry offset plate/run prices.
    let (status, body) = app
        .pricing_create(
            &org,
            "machines",
            json!({
                "name": "Bad digital", "technology": "digital",
                "sheet_size_mm": [320, 450], "duplex": true, "max_grammage_gsm": 350,
                "setup_micro": 2000000, "click_mono_micro": 8000, "click_color_micro": 60000,
                "plate_price_micro": 5000, "waste_fixed_sheets": 10, "waste_percent": 2
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"]["details"]["plate_price_micro"]["code"],
        "not_for_digital"
    );

    // A version is not consumed by a rejected write.
    assert_eq!(app.pricing_version(&org).await, 1);
}

#[tokio::test]
#[ignore]
async fn catalogs_are_isolated_between_tenants() {
    let app = TestApp::spawn().await;

    let (status, created) = app
        .pricing_create(
            "iso-a",
            "formats",
            json!({ "name": "A4", "trim_mm": [210, 297] }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_string();

    assert_eq!(
        app.pricing_list("iso-a", "formats").await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        app.pricing_list("iso-b", "formats").await["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let (status, _) = app.pricing_get("iso-b", "formats", &id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.pricing_get("iso-a", "formats", &id).await;
    assert_eq!(status, StatusCode::OK);
}

/// Seed the five catalog tables from the golden fixture with their canonical
/// ids, pinning the version to 1 (§9.6).
async fn seed_golden(session: &surrealdb::Surreal<surrealdb::engine::any::Any>) {
    let dataset: Value = serde_json::from_str(GOLDEN).unwrap();
    for (array_key, table) in CATALOG_TABLES {
        for mut row in dataset[array_key].as_array().cloned().unwrap_or_default() {
            let id = row["id"].as_str().unwrap().to_string();
            let key = id
                .strip_prefix(&format!("{table}:"))
                .unwrap_or(&id)
                .to_string();
            row.as_object_mut().unwrap().remove("id");
            session
                .query("UPSERT type::record($tb, $key) CONTENT $content")
                .bind(("tb", *table))
                .bind(("key", key))
                .bind(("content", row))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
    }
    session
        .query("UPSERT meta:pricing SET version = 1")
        .await
        .unwrap()
        .check()
        .unwrap();
}

/// The §9.3 resolved JobSpec at a given quantity.
fn golden_job(quantity: u32) -> JobSpec {
    serde_json::from_value(json!({
        "format": "format:a5",
        "quantity": quantity,
        "components": [
            { "role": "cover",    "pages": 2,   "colors": "4/0", "material": "material:gloss_300" },
            { "role": "interior", "pages": 100, "colors": "4/0", "material": "material:offset_80" },
            { "role": "backing",  "pages": 2,   "colors": "0/0", "material": "material:board_500" }
        ],
        "operations": [
            { "operation": "operation:spiral_binding", "params": { "material": "material:spiral_black" } },
            { "operation": "operation:cutting",  "params": {} },
            { "operation": "operation:prepress", "params": {} }
        ]
    }))
    .unwrap()
}

#[tokio::test]
#[ignore]
async fn snapshot_prices_golden_dataset_and_rebuilds_on_mutation() {
    let db = shared_db().await;
    let store = Arc::new(
        Store::connect(&DbConfig {
            url: db.url().to_string(),
            user: "root".to_string(),
            pass: "root".to_string(),
            ns: ulid::Ulid::new().to_string(),
        })
        .await
        .expect("store connect"),
    );
    let provisioner = TenantProvisioner::new(store.clone());
    let tenant = provisioner
        .ensure_tenant("snap-org", "Snapshot demo")
        .await
        .expect("provision tenant");
    let session = store
        .for_tenant(&tenant.db_name)
        .await
        .expect("tenant session");
    seed_golden(&session).await;

    let repo = SurrealPricingRepo::new(Arc::clone(&session));
    let cache = PriceModelCache::new();

    let model = cache
        .get(&repo, &tenant.db_name)
        .await
        .expect("build snapshot");
    assert_eq!(model.pricelist_version, 1);

    let job = golden_job(100);
    let policy = model
        .pricing_policies
        .get("pricing_policy:standard")
        .unwrap();
    let breakdown = price_job(&model, policy, &job, None).expect("price golden job");
    // §9.4 normative numbers.
    assert_eq!(breakdown.cost_micro, 170_755_000);
    assert_eq!(breakdown.total_minor, 29_030);

    // Mutate the interior stock's per-sheet price and confirm the snapshot
    // rebuilds under a new version without a restart.
    let mut material = repo
        .get(PricingEntity::Material, "material:offset_80")
        .await
        .unwrap()
        .expect("offset_80 exists");
    material["pricing"]["price_micro"] = json!(80000);
    repo.update(PricingEntity::Material, "material:offset_80", material)
        .await
        .expect("update material price");

    let rebuilt = cache
        .get(&repo, &tenant.db_name)
        .await
        .expect("rebuild snapshot");
    assert_eq!(rebuilt.pricelist_version, 2);
    assert_eq!(
        rebuilt.materials["material:offset_80"]
            .pricing
            .price_micro(),
        80000
    );
    let rebuilt_breakdown = price_job(
        &rebuilt,
        rebuilt
            .pricing_policies
            .get("pricing_policy:standard")
            .unwrap(),
        &job,
        None,
    )
    .expect("re-price");
    assert!(rebuilt_breakdown.total_minor > breakdown.total_minor);
}
