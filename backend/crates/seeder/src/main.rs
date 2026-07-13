//! Fake-data generator for the demo tenant (PLAN.md M2: "seeder crate
//! producing customers and orders on the demo tenant, batched
//! inserts"). Run with `just seed` or `cargo run -p seeder`.

mod ua;

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
const UA_LOCALE: &str = "ua";

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

fn random_line_items(rng: &mut impl Rng, currency: &str, products: &[&str]) -> Vec<LineItem> {
    let count = rng.gen_range(1..=4);
    (0..count)
        .map(|_| LineItem {
            description: (*products.choose(rng).unwrap()).to_string(),
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

    let locale = env_or("SEED_LOCALE", "en");
    let ukrainian = locale == UA_LOCALE;

    let customer_count = env_count("SEED_CUSTOMERS", if ukrainian { 100 } else { 10_000 });
    let order_count = env_count("SEED_ORDERS", if ukrainian { 1_000 } else { 100_000 });
    let default_org_id = if ukrainian { "demo-ua" } else { "demo" };
    let default_org_name = if ukrainian {
        "Демо Друкарня"
    } else {
        "Demo Print Shop"
    };
    let org_id = env_or("SEED_ORG_ID", default_org_id);
    let org_name = env_or("SEED_ORG_NAME", default_org_name);

    let config = DbConfig {
        url: env_or("SURREALDB_URL", "ws://localhost:8000"),
        user: env_or("SURREALDB_USER", "root"),
        pass: env_or("SURREALDB_PASS", "root"),
        ns: env_or("SURREALDB_NS", "polymix"),
    };

    let store = Arc::new(Store::connect(&config).await?);
    let provisioner = TenantProvisioner::new(store.clone());
    let tenant = if ukrainian {
        provisioner
            .provision_with_locale(&org_id, &org_name, "ua", "UAH")
            .await?
    } else {
        provisioner.ensure_tenant(&org_id, &org_name).await?
    };
    let session = store.for_tenant(&tenant.db_name).await?;

    tracing::info!(tenant = %tenant.db_name, org_id = %org_id, "seeding tenant");

    let started = Instant::now();
    let customer_ids = seed_customers(&session, customer_count, ukrainian).await?;
    tracing::info!(count = customer_ids.len(), elapsed = ?started.elapsed(), "customers seeded");

    let orders_started = Instant::now();
    seed_orders(
        &session,
        &customer_ids,
        order_count,
        &tenant.default_currency,
        ukrainian,
    )
    .await?;
    tracing::info!(count = order_count, elapsed = ?orders_started.elapsed(), "orders seeded");

    tracing::info!(elapsed = ?started.elapsed(), "seed complete");
    Ok(())
}

async fn seed_customers(
    session: &Surreal<Any>,
    count: usize,
    ukrainian: bool,
) -> anyhow::Result<Vec<String>> {
    let mut rng = rand::thread_rng();
    let mut ids = Vec::with_capacity(count);
    let mut remaining = count;
    let mut seq = 0usize;

    while remaining > 0 {
        let batch_size = remaining.min(BATCH_SIZE);
        let now = chrono::Utc::now().to_rfc3339();
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let id = Ulid::new().to_string();
            batch.push(if ukrainian {
                CustomerSeedRow {
                    id: RecordId::new(CUSTOMER_TABLE, id.clone()),
                    name: ua::company_name(&mut rng),
                    contact_name: Some(ua::contact_name(&mut rng)),
                    email: Some(ua::email(&mut rng, seq)),
                    phone: Some(ua::phone(&mut rng)),
                    address: Some(AddressRow {
                        street: Some(ua::street(&mut rng)),
                        zip: Some(ua::zip(&mut rng)),
                        city: Some(ua::city(&mut rng)),
                        country: Some("UA".to_string()),
                    }),
                    notes: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                }
            } else {
                CustomerSeedRow {
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
                }
            });
            ids.push(id);
            seq += 1;
        }

        let _: Vec<CustomerSeedRow> = session.insert(CUSTOMER_TABLE).content(batch).await?;
        remaining -= batch_size;
        tracing::info!(seeded = ids.len(), count, "customers progress");
    }

    Ok(ids)
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CounterRow {
    value: i64,
}

async fn seed_orders(
    session: &Surreal<Any>,
    customer_ids: &[String],
    count: usize,
    currency: &str,
    ukrainian: bool,
) -> anyhow::Result<()> {
    let mut rng = rand::thread_rng();
    let mut remaining = count;
    let mut seeded = 0usize;
    let products = if ukrainian { ua::PRODUCTS } else { PRODUCTS };

    // Re-running the seeder against an already-provisioned tenant must not
    // restart numbering at 000001: that would mint duplicates against
    // existing orders and rewind the shared counter, colliding with the
    // next API-created order.
    let mut response = session
        .query("SELECT `value` FROM counter:order")
        .await?
        .check()?;
    let rows: Vec<CounterRow> = response.take(0)?;
    let mut number = rows.first().map(|r| r.value).unwrap_or(0) as u64;

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
            let line_items = random_line_items(&mut rng, currency, products);
            let total = line_items_total(&line_items, currency);
            let status = random_status(&mut rng);
            batch.push(OrderSeedRow {
                id: RecordId::new(ORDER_TABLE, id),
                // Matches `counter::next_number`'s empty-prefix format — the
                // tenant's `order_prefix` defaults to empty (PLAN.md M4).
                number: format!("{number:06}"),
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
    // through the API would collide. `math::max` guards against rewinding
    // the counter below its pre-run value.
    session
        .query("UPSERT counter:order SET `value` = math::max([`value` ?? 0, $n])")
        .bind(("n", number as i64))
        .await?
        .check()?;

    Ok(())
}
