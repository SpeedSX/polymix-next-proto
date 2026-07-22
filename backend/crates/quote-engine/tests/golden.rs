//! §9 golden fixture + §10 tests 1, 6, 7 (fixture-based).

mod common;

use common::*;
use quote_engine::{price_at, quote_template};

/// §10 test 1 — end-to-end §9.4/§9.5 numbers, including intermediate
/// `cost_micro` per component and operation.
#[test]
fn golden_breakdown_matches_spec() {
    let model = demo_model();
    let template = demo_template(&model);
    let selection = golden_selection();
    let expected = expected();

    for qty_key in ["100", "1000"] {
        let qty: u32 = qty_key.parse().unwrap();
        let breakdown = price_at(&model, &template, &selection, qty, None).unwrap();
        let want = &expected["breakdowns"][qty_key];

        for component in &breakdown.components {
            let want_cost = want["components"][component.role.as_str()]
                .as_i64()
                .unwrap_or_else(|| panic!("expected cost for component {}", component.role));
            assert_eq!(
                component.cost_micro, want_cost,
                "qty {qty}: component {} cost_micro",
                component.role
            );
        }
        for operation in &breakdown.operations {
            let want_cost = want["operations"][operation.operation.as_str()]
                .as_i64()
                .unwrap_or_else(|| panic!("expected cost for op {}", operation.operation));
            assert_eq!(
                operation.cost_micro, want_cost,
                "qty {qty}: operation {} cost_micro",
                operation.operation
            );
        }

        assert_eq!(
            breakdown.cost_micro,
            want["cost_micro"].as_i64().unwrap(),
            "qty {qty}: cost_micro"
        );
        assert_eq!(
            breakdown.total_minor,
            want["total_minor"].as_i64().unwrap(),
            "qty {qty}: total_minor"
        );
        assert_eq!(
            breakdown.unit_minor,
            want["unit_minor"].as_i64().unwrap(),
            "qty {qty}: unit_minor"
        );
    }
}

#[test]
fn golden_ladder_entries_match() {
    let model = demo_model();
    let template = demo_template(&model);
    let quote = quote_template(&model, &template, &golden_selection(), None, None).unwrap();

    assert_eq!(quote.pricelist_version, 1);
    assert_eq!(quote.currency, "EUR");

    let entry = |qty: u32| {
        quote
            .ladder
            .iter()
            .find(|e| e.qty == qty)
            .unwrap_or_else(|| panic!("ladder entry for {qty}"))
    };
    assert_eq!(
        (entry(100).total_minor, entry(100).unit_minor),
        (29030, 290)
    );
    assert_eq!(
        (entry(1000).total_minor, entry(1000).unit_minor),
        (160080, 160)
    );
    assert_eq!(quote.ladder.len(), 7, "full template ladder");
}

/// §10 test 6 — fixture ladder monotonicity, as the invariants that actually
/// hold on the §9 dataset (see docs/adr/0013): `total_minor` non-decreasing
/// *within* a margin band (a decrease is permitted only at a band boundary,
/// where a bulk discount can lower a larger order's total), and `unit_minor`
/// non-increasing across the template ladder quantities.
#[test]
fn fixture_ladder_is_monotonic() {
    let model = demo_model();
    let template = demo_template(&model);
    let selection = golden_selection();
    let policy = &model.pricing_policies["pricing_policy:standard"];
    let band = |qty: u32| policy.band_for(qty).map(|b| b.multiplier_bp);

    let mut prev: Option<(u32, i64)> = None;
    for qty in (50..=2000).step_by(50) {
        let total = price_at(&model, &template, &selection, qty, None)
            .unwrap()
            .total_minor;
        if let Some((prev_qty, prev_total)) = prev
            && band(prev_qty) == band(qty)
        {
            assert!(
                total >= prev_total,
                "total_minor dropped within a band at qty {qty}: {total} < {prev_total}"
            );
        }
        prev = Some((qty, total));
    }

    let mut prev_unit = i64::MAX;
    for &qty in &template.quantities {
        let unit = price_at(&model, &template, &selection, qty, None)
            .unwrap()
            .unit_minor;
        assert!(
            unit <= prev_unit,
            "unit_minor rose at qty {qty}: {unit} > {prev_unit}"
        );
        prev_unit = unit;
    }
}

/// The documented cross-band anomaly (docs/adr/0013): ordering more can lower
/// the total at a margin-band boundary. Pinned so the behavior is intentional.
#[test]
fn total_may_drop_at_a_margin_band_boundary() {
    let model = demo_model();
    let template = demo_template(&model);
    let selection = golden_selection();
    let total = |qty| {
        price_at(&model, &template, &selection, qty, None)
            .unwrap()
            .total_minor
    };
    assert!(
        total(1000) < total(950),
        "expected the ×1.6→×1.5 band boundary to lower the total"
    );
}

/// §10 test 7 — same request twice yields byte-identical output.
#[test]
fn pricing_is_deterministic() {
    let model = demo_model();
    let template = demo_template(&model);
    let selection = golden_selection();

    let first = quote_template(&model, &template, &selection, None, None).unwrap();
    let second = quote_template(&model, &template, &selection, None, None).unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
}
