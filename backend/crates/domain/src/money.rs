use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Money {
    #[validate(range(min = 0, code = "not_negative"))]
    pub amount_minor: i64,
    #[validate(length(equal = 3, code = "invalid_currency_code"))]
    pub currency: String,
}

impl Money {
    pub fn zero(currency: impl Into<String>) -> Self {
        Self {
            amount_minor: 0,
            currency: currency.into(),
        }
    }
}

/// Rounds `numerator / denominator` half-up. Used for tax:
/// `round(net_total × tax_rate_bp / 10000)`, half-up, per PLAN.md.
/// Callers pass non-negative operands only (money amounts and basis points
/// are never negative in this domain), so the `numerator + denominator/2`
/// trick is safe without a sign check.
pub fn round_half_up(numerator: i64, denominator: i64) -> i64 {
    (numerator + denominator / 2) / denominator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_half_up_rounds_up_at_exact_half() {
        assert_eq!(round_half_up(5, 2), 3);
    }

    #[test]
    fn round_half_up_rounds_down_below_half() {
        assert_eq!(round_half_up(4, 10), 0);
        assert_eq!(round_half_up(24, 10), 2);
    }

    #[test]
    fn negative_amount_is_invalid() {
        let money = Money {
            amount_minor: -1,
            currency: "EUR".to_string(),
        };
        assert!(money.validate().is_err());
    }

    #[test]
    fn short_currency_code_is_invalid() {
        let money = Money {
            amount_minor: 100,
            currency: "EU".to_string(),
        };
        assert!(money.validate().is_err());
    }
}
