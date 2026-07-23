# 0015 — Production plan snapshots onto the job, not followed through the quote link

## Status

Accepted.

## Context

`docs/staff-quoting.md` §"Quote → order" states, for the engine-priced lines of
an accepted quote:

> The engine-priced lines' `job_spec` + `breakdown` snapshots stay on the quote
> — they are the production plan A6 will consume; v1 does not copy them onto the
> order, it follows the link.

That is, production (A6) and materials (A7) would reach the physical plan by
navigating `order.quote` → the matching quote line → its `job_spec` +
`breakdown`, reading the snapshots live off the quote.

Designing the target Order/production model (`docs/order-production-design.md`)
surfaced three problems with reading the plan live off the quote:

1. **Quotes mutate; a committed job must not.** A sent quote is immutable, but
   it can be **cloned** into a new draft (the revision mechanism), and the price
   model it referenced changes over time. The link identifies the quote, but the
   production plan a job executes must be pinned to exactly what was confirmed at
   order time — independent of any later revision activity around that quote.
2. **The link is document-level and the conversion is lossy.** Conversion splits
   an engine line into one or two order `LineItem`s (residual-minor allocation)
   and synthesizes plain-string descriptions. `order.quote` yields the whole
   quote, not "the plan for this line," and there is no line-level path back to
   the originating `job_spec`.
3. **Production and materials are hot read paths.** The A6 board and A7 MRP
   would join through the quote on every render/scan to reach data that never
   changes after commit.

## Decision

At Order confirm (`Draft → Confirmed`), **snapshot** each engine-priced quote
line's `job_spec`, `breakdown`, and `pricelist_version` onto a dedicated
`production_job` entity (one per engine-priced quote line). The job owns two
halves:

- **frozen** — the as-quoted `job_spec` + `quoted_breakdown` + `pricelist_version`,
  never mutated (reference, and the A8 margin baseline);
- **mutable** — the as-planned `operations` and `materials` that production
  executes and re-plans.

Production and materials read the self-contained job, **not** the quote. The
`quote` ↔ `order` document link is kept, and the job carries a `quote_line_ref`
(`{ quote, line_id }`), purely for traceability. A new stable `line_id` on each
embedded quote line makes that reference exact; order lines also gain
`source: Option<QuoteLineRef>` so a billing line points back to its origin.

This overrides the "does not copy onto the order, follows the link" clause. Note
the snapshot lands on the **job**, not the order — `Order` stays a purely
commercial document with no physical facts, consistent with the rest of the
design.

## Consequences

- A quote clone or a price-model change after confirm cannot alter a committed
  job. The job is reproducible and auditable on its own.
- A6/A7 read one entity with no join through the quote; the engine's machine
  pick seeds `planned_machine` as a *suggestion* production can override
  (the pick is a pricing decision, not a scheduling commitment).
- A8 compares each job's frozen `quoted_breakdown` against actual consumed
  materials and machine time — a stable per-job baseline.
- Data is duplicated between the quote line and the job by design; the two serve
  different masters (immutable commercial record vs. mutable production plan) and
  are reconciled only through `quote_line_ref`.
- `docs/staff-quoting.md` §"Quote → order" is amended to match; until the
  `production_job` entity is built (A6), no code depends on this — Step 3 only
  adds the `line_id` / `source` link.
