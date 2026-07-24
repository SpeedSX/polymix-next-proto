# 0013 — Quote engine: total price is not monotonic across margin bands

## Status

Accepted.

## Context

`docs/quote-engine-spec.md` §10 test 6 requires, on the §9 golden dataset,
that `total_minor` be **non-decreasing** for qty 50..=2000 step 50. Implementing
the engine surfaced that this cannot hold together with the rest of the spec.

The §9 pricing policy has decreasing margin bands (×1.7 for qty 1–249, ×1.6 for
250–999, ×1.5 for 1000+), and §6.4 computes `total = round(cost × band_multiplier)`
with no clamp. At the 999→1000 boundary the 6.25 % margin cut (1.6→1.5) outweighs
the ~5 % cost increase from the extra copies, so the total **drops**:

- qty 950: `total_minor` = 162 830
- qty 1000: `total_minor` = 160 080  ← the normative §9.6 number

The two spec clauses are therefore mutually contradictory on this dataset:

- §9.4/§9.6 fix `total_minor` at qty 1000 to **160 080** (marked *normative — a
  mismatch is an engine bug*).
- §10 test 6 requires `total_minor(1000) ≥ total_minor(950) = 162 830`.

Both cannot be true. Any clamp that made totals monotonic (e.g.
`max(total, previous_total)`) would force qty 1000 to 162 830 and break the
normative golden number.

## Decision

The §9 golden numbers win — they are explicitly normative, and cross-band total
drops are a real property of decreasing-margin bulk pricing, not a bug. The
engine implements §6.4 exactly, with no monotonicity clamp.

§10 test 6 is implemented as the invariants that actually hold and that carry the
intended economic meaning:

- `unit_minor` is **non-increasing** across the template's ladder quantities
  (bulk is cheaper per unit) — kept as-is.
- `total_minor` is **non-decreasing within a single margin band**; a decrease is
  permitted only at a band boundary, where a bulk discount can lower the total of
  a larger order.

The engine does not attempt to hide the "order more, pay less" anomaly at band
boundaries. Whether to clamp it (so a customer is never quoted more for fewer
copies) is a pricing-policy decision for the admin/quote UI layer, recorded here
as a known follow-up — not an engine change, since clamping in the engine would
break the normative fixture.

## Consequences

- `docs/quote-engine-spec.md` §10 test 6 should be amended to the within-band
  wording above; until then, this ADR is the source of truth for the test.
- A future pricing-policy option ("never quote a higher total for fewer copies")
  can be layered above the engine without touching §6.4 or the golden fixture.
