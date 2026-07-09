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
/// "invoice") via an atomic UPSERT (`counter:<kind>`), formatted as
/// `<prefix>-NNNNNN`, or bare `NNNNNN` when `prefix` is empty — the tenant's
/// `order_prefix`/`invoice_prefix` default to empty (PLAN.md M4: "default is
/// empty so no prefix displayed, just number").
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
    Ok(format_number(prefix, value))
}

fn format_number(prefix: &str, value: i64) -> String {
    if prefix.is_empty() {
        format!("{value:06}")
    } else {
        format!("{prefix}-{value:06}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_without_a_dash_when_prefix_is_empty() {
        assert_eq!(format_number("", 123), "000123");
    }

    #[test]
    fn formats_with_the_prefix_when_set() {
        assert_eq!(format_number("ORD", 123), "ORD-000123");
    }
}
