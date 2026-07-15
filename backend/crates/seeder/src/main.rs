//! Fake-data generator for the demo tenant (PLAN.md M2: "seeder crate
//! producing customers and orders on the demo tenant, batched
//! inserts"). Run with `just seed` or `cargo run -p seeder`.

mod ua;

use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use std::time::Instant;

use domain::customer::{CustomerKind, CustomerStatus, can_order};
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

// ~60% legal entity, ~35% ФОП, ~5% private individual (docs/customers-crm.md
// Step 6) — used for both demo tenants so the perf tenant's FTS index sees
// the same field-value distribution as the Ukrainian one.
const CUSTOMER_KIND_WEIGHTS: &[(CustomerKind, u32)] = &[
    (CustomerKind::LegalEntity, 60),
    (CustomerKind::Fop, 35),
    (CustomerKind::Individual, 5),
];

// Mostly active, with a deliberate minority of every other status so the
// list's status filter has something to demo.
const CUSTOMER_STATUS_WEIGHTS: &[(CustomerStatus, u32)] = &[
    (CustomerStatus::Active, 70),
    (CustomerStatus::Lead, 10),
    (CustomerStatus::Inactive, 10),
    (CustomerStatus::Blocked, 10),
];

const PAYMENT_TERMS_OPTIONS: &[u16] = &[0, 7, 14, 30];

const EN_TAGS: &[&str] = &["printing", "regular", "wholesale", "new", "vip"];
const EN_CONTACT_ROLES: &[&str] = &["director", "purchasing manager", "accountant"];

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
struct ContactRow {
    name: String,
    role: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    is_primary: bool,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CustomerSeedRow {
    id: RecordId,
    kind: i64,
    name: String,
    legal_name: Option<String>,
    edrpou: Option<String>,
    tax_id: Option<String>,
    vat_ipn: Option<String>,
    status: i64,
    tags: Vec<String>,
    industry: Option<String>,
    source: Option<String>,
    website: Option<String>,
    contacts: Vec<ContactRow>,
    legal_address: Option<AddressRow>,
    delivery_address: Option<AddressRow>,
    payment_terms_days: u16,
    credit_limit: Option<MoneyRow>,
    default_currency: String,
    default_discount_bp: u16,
    iban: Option<String>,
    bank_name: Option<String>,
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
    status: i64,
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

fn weighted_pick<T: Copy>(rng: &mut impl Rng, weights: &[(T, u32)]) -> T {
    let total: u32 = weights.iter().map(|(_, weight)| weight).sum();
    let mut pick = rng.gen_range(0..total);
    for (value, weight) in weights {
        if pick < *weight {
            return *value;
        }
        pick -= weight;
    }
    unreachable!("weights partition the full range")
}

fn random_digits(rng: &mut impl Rng, len: usize) -> String {
    (0..len)
        .map(|_| char::from_digit(rng.gen_range(0..10), 10).unwrap())
        .collect()
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
    let customers = seed_customers(
        &session,
        customer_count,
        ukrainian,
        &tenant.default_currency,
    )
    .await?;
    tracing::info!(count = customers.len(), elapsed = ?started.elapsed(), "customers seeded");

    let orders_started = Instant::now();
    seed_orders(
        &session,
        &customers,
        order_count,
        &tenant.default_currency,
        ukrainian,
    )
    .await?;
    tracing::info!(count = order_count, elapsed = ?orders_started.elapsed(), "orders seeded");

    tracing::info!(elapsed = ?started.elapsed(), "seed complete");
    Ok(())
}

fn seed_contacts(rng: &mut impl Rng, ukrainian: bool, seq: usize) -> Vec<ContactRow> {
    let count = rng.gen_range(1..=3);
    (0..count)
        .map(|i| {
            let (name, role, email, phone) = if ukrainian {
                (
                    ua::contact_name(rng),
                    ua::CONTACT_ROLES.choose(rng).unwrap().to_string(),
                    ua::email(rng, seq * 10 + i),
                    ua::phone(rng),
                )
            } else {
                (
                    Name().fake_with_rng(rng),
                    EN_CONTACT_ROLES.choose(rng).unwrap().to_string(),
                    FreeEmail().fake_with_rng(rng),
                    PhoneNumber().fake_with_rng(rng),
                )
            };
            ContactRow {
                name,
                role: Some(role),
                email: Some(email),
                phone: Some(phone),
                is_primary: i == 0,
            }
        })
        .collect()
}

fn seed_tags(rng: &mut impl Rng, ukrainian: bool) -> Vec<String> {
    let pool = if ukrainian { ua::TAGS } else { EN_TAGS };
    let count = rng.gen_range(0..=3);
    let mut chosen: Vec<String> = pool
        .choose_multiple(rng, count)
        .map(|s| s.to_string())
        .collect();
    chosen.sort();
    chosen.dedup();
    chosen
}

/// Customer id + status as written during seeding — order selection needs the
/// status so it can apply the same `can_order` / lead-promote rules as the API.
struct SeededCustomer {
    id: String,
    status: CustomerStatus,
}

async fn seed_customers(
    session: &Surreal<Any>,
    count: usize,
    ukrainian: bool,
    default_currency: &str,
) -> anyhow::Result<Vec<SeededCustomer>> {
    let mut rng = rand::thread_rng();
    let mut customers = Vec::with_capacity(count);
    let mut remaining = count;
    let mut seq = 0usize;

    while remaining > 0 {
        let batch_size = remaining.min(BATCH_SIZE);
        let now = chrono::Utc::now().to_rfc3339();
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let id = Ulid::new().to_string();
            let kind = weighted_pick(&mut rng, CUSTOMER_KIND_WEIGHTS);
            let edrpou = (kind == CustomerKind::LegalEntity).then(|| random_digits(&mut rng, 8));
            let tax_id = (kind != CustomerKind::LegalEntity).then(|| random_digits(&mut rng, 10));
            let vat_ipn = rng.gen_bool(0.4).then(|| random_digits(&mut rng, 12));
            // Weighted mix is intentional for the list/status filter demo;
            // ineligible statuses are filtered out again in `seed_orders`.
            let status = weighted_pick(&mut rng, CUSTOMER_STATUS_WEIGHTS);
            let payment_terms_days = *PAYMENT_TERMS_OPTIONS.choose(&mut rng).unwrap();
            let contacts = seed_contacts(&mut rng, ukrainian, seq);
            let tags = seed_tags(&mut rng, ukrainian);

            let (name, legal_address) = if ukrainian {
                (
                    ua::company_name(&mut rng),
                    AddressRow {
                        street: Some(ua::street(&mut rng)),
                        zip: Some(ua::zip(&mut rng)),
                        city: Some(ua::city(&mut rng)),
                        country: Some("UA".to_string()),
                    },
                )
            } else {
                (
                    CompanyName().fake_with_rng(&mut rng),
                    AddressRow {
                        street: Some(StreetName().fake_with_rng(&mut rng)),
                        zip: Some(ZipCode().fake_with_rng(&mut rng)),
                        city: Some(CityName().fake_with_rng(&mut rng)),
                        country: Some(CountryCode().fake_with_rng(&mut rng)),
                    },
                )
            };

            batch.push(CustomerSeedRow {
                id: RecordId::new(CUSTOMER_TABLE, id.clone()),
                kind: kind.code() as i64,
                name,
                legal_name: None,
                edrpou,
                tax_id,
                vat_ipn,
                status: status.code() as i64,
                tags,
                industry: None,
                source: None,
                website: None,
                contacts,
                legal_address: Some(legal_address),
                delivery_address: None,
                payment_terms_days,
                credit_limit: None,
                default_currency: default_currency.to_string(),
                default_discount_bp: 0,
                iban: None,
                bank_name: None,
                notes: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
            customers.push(SeededCustomer { id, status });
            seq += 1;
        }

        let _: Vec<CustomerSeedRow> = session.insert(CUSTOMER_TABLE).content(batch).await?;
        remaining -= batch_size;
        tracing::info!(seeded = customers.len(), count, "customers progress");
    }

    Ok(customers)
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CounterRow {
    value: i64,
}

async fn seed_orders(
    session: &Surreal<Any>,
    customers: &[SeededCustomer],
    count: usize,
    currency: &str,
    ukrainian: bool,
) -> anyhow::Result<()> {
    let mut rng = rand::thread_rng();
    let mut remaining = count;
    let mut seeded = 0usize;
    let products = if ukrainian { ua::PRODUCTS } else { PRODUCTS };

    // Same eligibility as `OrderRepo::create`: only lead/active may receive
    // orders. Inactive/blocked stay in the customer mix for list demos but
    // are never attached to seeded orders.
    let eligible: Vec<&str> = customers
        .iter()
        .filter(|c| can_order(c.status))
        .map(|c| c.id.as_str())
        .collect();
    anyhow::ensure!(
        !eligible.is_empty(),
        "no order-eligible customers (need at least one lead or active)"
    );

    // Leads that receive an order are promoted to active — mirrors the
    // conversion event in `promote_customer_to_active`.
    let mut pending_leads: HashSet<&str> = customers
        .iter()
        .filter(|c| c.status == CustomerStatus::Lead)
        .map(|c| c.id.as_str())
        .collect();
    let mut promoted_leads: Vec<String> = Vec::new();

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
            let customer_id =
                (*eligible.choose(&mut rng).expect("eligible is non-empty")).to_string();
            if pending_leads.remove(customer_id.as_str()) {
                promoted_leads.push(customer_id.clone());
            }
            let line_items = random_line_items(&mut rng, currency, products);
            let total = line_items_total(&line_items, currency);
            let status = random_status(&mut rng);
            batch.push(OrderSeedRow {
                id: RecordId::new(ORDER_TABLE, id),
                // Matches `counter::next_number`'s empty-prefix format — the
                // tenant's `order_prefix` defaults to empty (PLAN.md M4).
                number: format!("{number:06}"),
                customer_id,
                status: status.code() as i64,
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

    promote_ordered_leads(session, &promoted_leads).await?;

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

async fn promote_ordered_leads(session: &Surreal<Any>, lead_ids: &[String]) -> anyhow::Result<()> {
    if lead_ids.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let active = CustomerStatus::Active.code() as i64;
    for chunk in lead_ids.chunks(BATCH_SIZE) {
        let ids: Vec<RecordId> = chunk
            .iter()
            .map(|id| RecordId::new(CUSTOMER_TABLE, id.clone()))
            .collect();
        session
            .query(
                "UPDATE customer SET status = $status, updated_at = $now WHERE id IN $ids RETURN NONE",
            )
            .bind(("status", active))
            .bind(("now", now.clone()))
            .bind(("ids", ids))
            .await?
            .check()?;
    }
    tracing::info!(
        count = lead_ids.len(),
        "leads promoted to active after first order"
    );
    Ok(())
}
