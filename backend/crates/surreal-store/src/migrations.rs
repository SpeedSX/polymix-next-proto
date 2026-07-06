use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::SurrealValue;

/// Ordered migrations, applied in this order. Add new entries at the end as
/// milestones introduce them — see PLAN.md's migration numbering rule.
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.surql")),
    (
        "0002_customers",
        include_str!("../migrations/0002_customers.surql"),
    ),
];

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct MigrationsMeta {
    version: i64,
}

pub async fn apply_migrations(session: &Surreal<Any>) -> surrealdb::Result<()> {
    // SurrealDB 3.x errors "table does not exist" on SELECT against a table
    // that was never created — define it eagerly since this runs against a
    // brand-new tenant db where `meta` has never been touched.
    session
        .query("DEFINE TABLE IF NOT EXISTS meta SCHEMALESS")
        .await?
        .check()?;
    let current: Option<MigrationsMeta> = session.select(("meta", "migrations")).await?;
    let applied = current.map(|m| m.version).unwrap_or(0);

    for (idx, (name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = (idx + 1) as i64;
        if version <= applied {
            continue;
        }
        tracing::info!(migration = name, "applying migration");
        session.query(*sql).await?.check()?;
        session
            .upsert::<Option<MigrationsMeta>>(("meta", "migrations"))
            .content(MigrationsMeta { version })
            .await?;
    }
    Ok(())
}
