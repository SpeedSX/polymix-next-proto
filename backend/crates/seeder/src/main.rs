//! Fake-data generator for the demo tenant (PLAN.md M2: "seeder crate
//! producing >= 50k customers / >= 200k orders per demo tenant, batched
//! inserts of 1000"). Run with `just seed` or `cargo run -p seeder`.

use std::env;
use std::sync::Arc;
use std::time::Instant;

use domain::money::Money;
use domain::order::{LineItem, OrderStatus, line_items_total};
use fake::Fake;
use fake::faker::address::en::{CityName, CountryCode, StreetName, ZipCode};
use fake::faker::company::en::CompanyName;
use fake::faker::internet::en::FreeEmail;
use fake::faker::name::en::Name;
use fake::faker::phone_number::en::PhoneNumber;
use rand::Rng;
use rand::seq::SliceRandom;
use surreal_store::{DbConfig, Store, TenantProvisioner};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, SurrealValue};
use ulid::Ulid;

const BATCH_SIZE: usize = 1000;
const CUSTOMER_TABLE: &str = "customer";
const ORDER_TABLE: &str = "order";

const PRODUCTS: &[&str] = &[
    "Business cards",
    "Flyers",
    "Brochures",
    "Posters",
    "Banners",
    "Letterheads",
    "Envelopes",
    "Stickers",
    "Postcards",
    "Catalogs",
];

// Rough distribution of a print shop's order book: most orders have already
// moved past draft, a healthy chunk are done, a few fell through.
const ORDER_STATUS_WEIGHTS: &[(OrderStatus, u32)] = &[
    (OrderStatus::Draft, 15),
    (OrderStatus::Confirmed, 20),
    (OrderStatus::InProduction, 20),
    (OrderStatus::Completed, 35),
    (OrderStatus::Cancelled, 10),
];

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct AddressRow {
    street: Option<String>,
    zip: Option<String>,
    city: Option<String>,
    country: Option<String>,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CustomerSeedRow {
    id: RecordId,
    name: String,
    contact_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    address: Option<AddressRow>,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct MoneyRow {
    amount_minor: i64,
    currency: String,
}

impl From<Money> for MoneyRow {
    fn from(money: Money) -> Self {
        Self {
            amount_minor: money.amount_minor,
            currency: money.currency,
        }
    }
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct LineItemRow {
    description: String,
    quantity: u32,
    unit_price: MoneyRow,
}

impl From<LineItem> for LineItemRow {
    fn from(item: LineItem) -> Self {
        Self {
            description: item.description,
            quantity: item.quantity,
            unit_price: item.unit_price.into(),
        }
    }
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct OrderSeedRow {
    id: RecordId,
    number: String,
    customer_id: String,
    status: String,
    currency: String,
    line_items: Vec<LineItemRow>,
    total: MoneyRow,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_count(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn status_str(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::Draft => "draft",
        OrderStatus::Confirmed => "confirmed",
        OrderStatus::InProduction => "in_production",
        OrderStatus::Completed => "completed",
        OrderStatus::Cancelled => "cancelled",
    }
}

fn random_status(rng: &mut impl Rng) -> OrderStatus {
    let total: u32 = ORDER_STATUS_WEIGHTS.iter().map(|(_, weight)| weight).sum();
    let mut pick = rng.gen_range(0..total);
    for (status, weight) in ORDER_STATUS_WEIGHTS {
        if pick < *weight {
            return *status;
        }
        pick -= weight;
    }
    unreachable!("weights partition the full range")
}

fn random_line_items(rng: &mut impl Rng, currency: &str) -> Vec<LineItem> {
    let count = rng.gen_range(1..=4);
    (0..count)
        .map(|_| LineItem {
            description: (*PRODUCTS.choose(rng).unwrap()).to_string(),
            quantity: rng.gen_range(1..=500),
            unit_price: Money {
                amount_minor: rng.gen_range(10..=25_000),
                currency: currency.to_string(),
            },
        })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let customer_count = env_count("SEED_CUSTOMERS", 50_000);
    let order_count = env_count("SEED_ORDERS", 200_000);
    let org_id = env_or("SEED_ORG_ID", "demo");
    let org_name = env_or("SEED_ORG_NAME", "Demo Print Shop");

    let config = DbConfig {
        url: env_or("SURREALDB_URL", "ws://localhost:8000"),
        user: env_or("SURREALDB_USER", "root"),
        pass: env_or("SURREALDB_PASS", "root"),
        ns: env_or("SURREALDB_NS", "polymix"),
    };

    let store = Arc::new(Store::connect(&config).await?);
    let provisioner = TenantProvisioner::new(store.clone());
    let tenant = provisioner.ensure_tenant(&org_id, &org_name).await?;
    let session = store.for_tenant(&tenant.db_name).await?;

    tracing::info!(tenant = %tenant.db_name, org_id = %org_id, "seeding tenant");

    let started = Instant::now();
    let customer_ids = seed_customers(&session, customer_count).await?;
    tracing::info!(count = customer_ids.len(), elapsed = ?started.elapsed(), "customers seeded");

    let orders_started = Instant::now();
    seed_orders(
        &session,
        &customer_ids,
        order_count,
        &tenant.default_currency,
    )
    .await?;
    tracing::info!(count = order_count, elapsed = ?orders_started.elapsed(), "orders seeded");

    tracing::info!(elapsed = ?started.elapsed(), "seed complete");
    Ok(())
}

async fn seed_customers(session: &Surreal<Any>, count: usize) -> anyhow::Result<Vec<String>> {
    let mut rng = rand::thread_rng();
    let mut ids = Vec::with_capacity(count);
    let mut remaining = count;

    while remaining > 0 {
        let batch_size = remaining.min(BATCH_SIZE);
        let now = chrono::Utc::now().to_rfc3339();
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let id = Ulid::new().to_string();
            batch.push(CustomerSeedRow {
                id: RecordId::new(CUSTOMER_TABLE, id.clone()),
                name: CompanyName().fake_with_rng(&mut rng),
                contact_name: Some(Name().fake_with_rng(&mut rng)),
                email: Some(FreeEmail().fake_with_rng(&mut rng)),
                phone: Some(PhoneNumber().fake_with_rng(&mut rng)),
                address: Some(AddressRow {
                    street: Some(StreetName().fake_with_rng(&mut rng)),
                    zip: Some(ZipCode().fake_with_rng(&mut rng)),
                    city: Some(CityName().fake_with_rng(&mut rng)),
                    country: Some(CountryCode().fake_with_rng(&mut rng)),
                }),
                notes: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
            ids.push(id);
        }

        let _: Vec<CustomerSeedRow> = session.insert(CUSTOMER_TABLE).content(batch).await?;
        remaining -= batch_size;
        tracing::info!(seeded = ids.len(), count, "customers progress");
    }

    Ok(ids)
}

async fn seed_orders(
    session: &Surreal<Any>,
    customer_ids: &[String],
    count: usize,
    currency: &str,
) -> anyhow::Result<()> {
    let mut rng = rand::thread_rng();
    let mut remaining = count;
    let mut seeded = 0usize;
    let mut number = 0u64;

    while remaining > 0 {
        let batch_size = remaining.min(BATCH_SIZE);
        let now = chrono::Utc::now().to_rfc3339();
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            number += 1;
            let id = Ulid::new().to_string();
            let customer_id = customer_ids
                .choose(&mut rng)
                .expect("customer_ids is non-empty")
                .clone();
            let line_items = random_line_items(&mut rng, currency);
            let total = line_items_total(&line_items, currency);
            let status = random_status(&mut rng);
            batch.push(OrderSeedRow {
                id: RecordId::new(ORDER_TABLE, id),
                number: format!("ORD-{number:06}"),
                customer_id,
                status: status_str(status).to_string(),
                currency: currency.to_string(),
                line_items: line_items.into_iter().map(LineItemRow::from).collect(),
                total: total.into(),
                notes: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }

        let _: Vec<OrderSeedRow> = session.insert(ORDER_TABLE).content(batch).await?;
        seeded += batch_size;
        remaining -= batch_size;
        tracing::info!(seeded, count, "orders progress");
    }

    // Orders above were assigned sequential numbers directly (bulk insert,
    // not the one-row-at-a-time `next_number` path), so the shared counter
    // needs to be caught up here or the first order created afterwards
    // through the API would collide with ORD-000001.
    session
        .query("UPSERT counter:order SET value = $n")
        .bind(("n", number as i64))
        .await?
        .check()?;

    Ok(())
}
