//! Small helpers shared across the repo modules — extracted so the
//! per-entity repos (`customer_repo`, `order_repo`, `invoice_repo`,
//! `tenant_repo`, …) don't each carry their own copy.

use std::collections::HashMap;

use domain::error::{DomainError, FieldError};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};

/// Maps a raw SurrealDB error into the domain's opaque store error.
pub(crate) fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

/// Extracts the string key portion of a `RecordId` (e.g. the `abc` in
/// `customer:abc`), falling back to the debug form for non-string keys.
pub(crate) fn record_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(key) => key.clone(),
        other => format!("{other:?}"),
    }
}

/// Treats an absent or empty search string as "no query".
pub(crate) fn non_empty_q(q: &Option<String>) -> Option<&str> {
    q.as_deref().filter(|s| !s.is_empty())
}

/// Turns a `sort` param (`field` for ASC, `-field` for DESC) into an
/// `ORDER BY` clause fragment, validating the field against `allowed`.
///
/// The field is whitelisted rather than bound as a query parameter:
/// SurrealQL identifiers (unlike values) can't be passed as bind
/// parameters, so it's interpolated into the query and must be validated
/// first.
pub(crate) fn sort_clause(sort: &str, allowed: &[&str]) -> Result<String, DomainError> {
    let (field, dir) = match sort.strip_prefix('-') {
        Some(field) => (field, "DESC"),
        None => (sort, "ASC"),
    };
    if !allowed.contains(&field) {
        let mut details = HashMap::new();
        details.insert(
            "sort".to_string(),
            FieldError::with_params(
                "unknown_sort_field",
                HashMap::from([("field".to_string(), field.to_string())]),
            ),
        );
        return Err(DomainError::Validation(details));
    }
    Ok(format!("{field} {dir}"))
}

/// Id-only projection, for existence checks and id-union queries.
#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct IdOnly {
    pub(crate) id: RecordId,
}

/// `SELECT count() ... GROUP ALL` projection.
#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct CountRow {
    pub(crate) count: i64,
}

/// Per-hit projection for the scalar-highlight search paths (orders,
/// invoices). `customer_repo` has its own richer variant (array highlights
/// plus a merge score).
#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct SearchHitRow {
    pub(crate) id: RecordId,
    pub(crate) label: String,
    pub(crate) highlight: Option<String>,
}

pub(crate) fn to_hit(row: SearchHitRow) -> domain::SearchHit {
    domain::SearchHit {
        id: record_key(&row.id),
        highlight: row.highlight.unwrap_or_else(|| row.label.clone()),
        label: row.label,
    }
}
