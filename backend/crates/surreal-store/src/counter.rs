use domain::error::DomainError;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::SurrealValue;

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CounterRow {
    value: i64,
}

fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

/// Assigns the next per-tenant sequence number for `kind` ("order" |
/// "invoice") via an atomic UPSERT (`counter:<kind>`), per PLAN.md's
/// document-numbering convention, formatted as `<prefix>-NNNNNN`.
///
/// `value` is backtick-escaped: confirmed empirically against
/// `surrealdb/surrealdb:v3.2` that it's a reserved SurrealQL keyword and
/// only parses as a bare field identifier when quoted (see
/// docs/surrealdb-rust-sdk-notes.md).
pub(crate) async fn next_number(
    session: &Surreal<Any>,
    kind: &str,
    prefix: &str,
) -> Result<String, DomainError> {
    let mut response = session
        .query("UPSERT type::record('counter', $kind) SET `value` += 1 RETURN `value`")
        .bind(("kind", kind.to_string()))
        .await
        .map_err(map_err)?
        .check()
        .map_err(map_err)?;
    let rows: Vec<CounterRow> = response.take(0).map_err(map_err)?;
    let value = rows
        .first()
        .map(|r| r.value)
        .ok_or_else(|| DomainError::Store("counter upsert returned no row".to_string()))?;
    Ok(format!("{prefix}-{value:06}"))
}
