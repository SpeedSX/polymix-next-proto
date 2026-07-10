//! Static exchange-rate table (PLAN.md M4): seeded once per tenant at
//! provisioning, read back for the invoice-creation snapshot and the
//! frontend's display-only converted amount. Never used for accounting —
//! see PLAN.md's "Out of scope" note on multi-currency accounting.

use domain::error::DomainError;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::SurrealValue;

const TABLE: &str = "exchange_rate";

/// Units of each currency per 1 EUR — the numeraire this static table is
/// built from. Illustrative prototype values, not live market rates.
const UNITS_PER_EUR: &[(&str, f64)] = &[
    ("EUR", 1.0),
    ("USD", 1.0842),
    ("GBP", 0.8420),
    ("UAH", 44.50),
];

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct ExchangeRateContent {
    base: String,
    quote: String,
    rate: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct RateRow {
    rate: String,
}

fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

/// `1 base = <rate> quote`, e.g. `compute_rate("EUR", "USD")` -> `Some("1.0842")`.
/// `None` when either currency isn't in `UNITS_PER_EUR` — an unsupported
/// currency simply gets no snapshot, since the feature is informational
/// only (PLAN.md: "Informational only — display conversions happen on the
/// frontend").
fn compute_rate(base: &str, quote: &str) -> Option<String> {
    let base_units = UNITS_PER_EUR.iter().find(|(code, _)| *code == base)?.1;
    let quote_units = UNITS_PER_EUR.iter().find(|(code, _)| *code == quote)?.1;
    Some(format!("{:.4}", quote_units / base_units))
}

/// Seeds `base = default_currency` rows against every other currency in
/// `UNITS_PER_EUR`, once, at tenant provisioning. A no-op when
/// `default_currency` itself isn't in the table.
pub async fn seed_default_rates(
    session: &Surreal<Any>,
    default_currency: &str,
) -> Result<(), DomainError> {
    let rows: Vec<ExchangeRateContent> = UNITS_PER_EUR
        .iter()
        .filter(|(quote, _)| *quote != default_currency)
        .filter_map(|(quote, _)| {
            compute_rate(default_currency, quote).map(|rate| ExchangeRateContent {
                base: default_currency.to_string(),
                quote: quote.to_string(),
                rate,
            })
        })
        .collect();

    if rows.is_empty() {
        return Ok(());
    }

    let _: Vec<ExchangeRateContent> = session.insert(TABLE).content(rows).await.map_err(map_err)?;
    Ok(())
}

/// `1 base = <rate> quote`, looked up for the invoice-creation snapshot.
/// `None` when no such pair was seeded for this tenant.
pub(crate) async fn lookup_rate(
    session: &Surreal<Any>,
    base: &str,
    quote: &str,
) -> Result<Option<String>, DomainError> {
    let mut response = session
        .query("SELECT rate FROM type::table($table) WHERE base = $base AND quote = $quote LIMIT 1")
        .bind(("table", TABLE))
        .bind(("base", base.to_string()))
        .bind(("quote", quote.to_string()))
        .await
        .map_err(map_err)?;
    let rows: Vec<RateRow> = response.take(0).map_err(map_err)?;
    Ok(rows.into_iter().next().map(|r| r.rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rate_is_one() {
        assert_eq!(compute_rate("EUR", "EUR"), Some("1.0000".to_string()));
    }

    #[test]
    fn eur_to_usd_matches_the_seeded_table() {
        assert_eq!(compute_rate("EUR", "USD"), Some("1.0842".to_string()));
    }

    #[test]
    fn inverting_base_and_quote_round_trips() {
        let eur_to_uah: f64 = compute_rate("EUR", "UAH").unwrap().parse().unwrap();
        let uah_to_eur: f64 = compute_rate("UAH", "EUR").unwrap().parse().unwrap();
        // Rates are stored to 4 decimal places, so a round trip through the
        // inverse pair carries that rounding error twice (~1.25e-3 for the
        // EUR/UAH magnitude) — 1e-6 is tighter than the format can deliver.
        assert!((eur_to_uah * uah_to_eur - 1.0).abs() < 3e-3);
    }

    #[test]
    fn unknown_currency_has_no_rate() {
        assert_eq!(compute_rate("EUR", "XYZ"), None);
        assert_eq!(compute_rate("XYZ", "EUR"), None);
    }
}
