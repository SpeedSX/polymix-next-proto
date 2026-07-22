//! §1 — integer money and geometry helpers.
//!
//! All engine money is `i64` micro-units (µ): `1 EUR = 1_000_000 µ`,
//! `1 minor unit (cent) = 10_000 µ`. No floats anywhere in the engine.

/// Bleed added per side when computing imposition footprints (§1). v1 has no
/// per-tenant override; the value is the industry-standard 3 mm trim bleed.
pub const BLEED_MM: u32 = 3;

/// Micro-units per minor unit (cent), per §1: `1 cent = 10_000 µ`.
pub const MICRO_PER_MINOR: i64 = 10_000;

/// `ceil(a / b)` for non-negative integers (§1). Callers pass positive
/// operands only.
pub fn ceil_div(a: i64, b: i64) -> i64 {
    (a + b - 1) / b
}

/// Divide `a` by `b` rounding a half up (§1). Callers pass positive operands
/// only, so the `+ b/2` bias is correct without a sign check.
pub fn round_half_up(a: i64, b: i64) -> i64 {
    (a + b / 2) / b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceil_div_rounds_up_on_remainder() {
        assert_eq!(ceil_div(5000, 4), 1250);
        assert_eq!(ceil_div(5001, 4), 1251);
        assert_eq!(ceil_div(100, 100), 1);
    }

    #[test]
    fn round_half_up_rounds_half_upward() {
        assert_eq!(round_half_up(29029, 10000), 3); // 2.9029 -> 3
        assert_eq!(round_half_up(5, 10), 1); // 0.5 -> 1
        assert_eq!(round_half_up(4, 10), 0); // 0.4 -> 0
        assert_eq!(round_half_up(29030, 100), 290); // exact
    }
}
