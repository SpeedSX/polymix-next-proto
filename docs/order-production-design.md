# Design: Order & Production model — commercial document vs. shop-floor plan

## Status

Proposed — not yet implemented. Supersedes the prototype `Order`
(`crates/domain/src/order.rs`), which was a billing-only stand-in with no
place for a production plan. Designed as the target structure for further
system development, ahead of A6 (production scheduling) and A7 (materials &
inventory) so those tracks build against the final shape rather than
retrofitting it.

Related docs: `docs/staff-quoting.md` (quote documents + quote→order
conversion — this design changes its conversion section, see §"Deviations"),
`docs/quote-engine-spec.md` (normative `JobSpec` and `Breakdown` shapes),
`docs/product-configuration.md` (effects → `JobSpec`), `PLAN.md` A4/A6/A7,
`docs/adr/0015-production-plan-snapshots-onto-job.md`.

## Context

The prototype `Order` is a commercial document and nothing more:

```
Order    { id, number, customer_id, status, currency, line_items, total, notes, … }
LineItem { description: String, quantity: u32, unit_price: Money }
```

A `LineItem` carries no physical facts — no format, material, colors, sheet
counts, machine, or operations. That is correct for what the prototype had to
prove (tenancy, search, live updates) but leaves two later tracks with nothing
to consume:

- **A6 — production scheduling / job board**: needs operations, machines,
  sheet counts, and a routing per job.
- **A7 — materials & inventory**: needs a per-job bill of materials to reserve
  and decrement stock against.

The physical facts *do* exist upstream — the engine produces them as the
`JobSpec` (`format`, `quantity`, `components[] {role, pages, colors,
material}`, `operations[] {operation, params}`, `technology_allow`) and the
`Breakdown` (per-component `{role, machine_id, sheets, cost_micro}`,
per-operation `{operation, cost_micro}`). `staff-quoting.md` stores both on the
accepted quote line. The gap is getting them to the shop floor in a form
production can actually plan and execute against.

## Design principle — one physical truth, snapshotted at each boundary

The engine runs **once** per priced line. Its output is *copied* — never
re-read live — at each point where the business commits to it:

```
engine ──produces──▶ quote line { JobSpec + Breakdown }   frozen when the quote is sent
                          │                                (what the customer was charged)
                          │ snapshot at Order confirm
                          ▼
                     production_job
                     ├─ frozen  : as-quoted JobSpec + Breakdown  (reference, A8 baseline)
                     └─ mutable : as-planned operations + materials
                          │
              ┌───────────┴───────────┐
              ▼                        ▼
        A6 job board             A7 materials / MRP
   (operations, machines,    (MaterialReservation:
    schedule — production      planned → reserved →
    re-plans freely)           consumed)
```

Snapshotting at boundaries keeps three concerns from contaminating each other:

- A **quote** can be cloned or revised, and the price model can change under it
  — a committed job must pin exactly what was confirmed.
- **Production** must re-plan (machine down, batching, capacity) without
  altering what the customer was quoted.
- **A8 margin** needs an immutable as-quoted cost to compare actual production
  cost against.

The engine's machine pick is a **pricing** decision (§6.2 keeps the cheapest
capable machine). It seeds the plan as a *suggestion*; production owns the
committed assignment.

## Entity 1 — `Order` (commercial document, reworked)

`Order` stays the billable agreement and the anchor invoices attach to. It
never carries physical facts. Two changes from the prototype:

```rust
struct OrderLine {
    // billing — unchanged intent
    description: String,
    quantity: u32,
    unit_price: Money,

    // provenance link; None for manual (tier-3) lines
    source: Option<QuoteLineRef>,
}

struct QuoteLineRef { quote: record<quote>, line_id: Uuid }
```

- **Stable `line_id` on every embedded quote line** (new — see the
  `staff-quoting.md` patch): an order line points back to the exact quote line
  that priced it, instead of a synthesized description that has to be
  reverse-matched.
- The billing residual-split (an engine line whose `final_total_minor` doesn't
  divide evenly becomes two order lines — `staff-quoting.md` §"Quote → order")
  is preserved, and **both** split lines carry the same `line_id`. Production
  keys off `line_id`, so a billing artifact never produces two jobs.

`OrderStatus` keeps its coarse commercial lifecycle
(`Draft → Confirmed → InProduction → Completed / Cancelled`). `InProduction`
and `Completed` become a **rollup** of the order's jobs — auto-advanced when
the first job is released / all jobs are done, with a staff override — rather
than a hand-flipped flag. `can_invoice` is unchanged (Confirmed onward).

## Entity 2 — `production_job` (new; one per engine-priced quote line)

Created when the order transitions `Draft → Confirmed`. One job per
engine-priced quote line (Template/Spec); manual lines produce **none**. A job
has a frozen half (as-quoted provenance) and a mutable half (as-planned):

```rust
struct ProductionJob {
    id, number,                       // shop-floor job number, tenant-scoped
    order: record<order>,
    quote_line_ref: QuoteLineRef,     // provenance (quote + line_id)
    status: JobStatus,                // queued → released → in_progress → done / cancelled

    // frozen — what the engine costed; reference + A8 baseline
    job_spec: JobSpec,                // immutable snapshot
    quoted_breakdown: Breakdown,      // immutable snapshot (the pricing machine pick)
    pricelist_version: i64,

    // mutable — what production executes
    operations: Vec<JobOperation>,    // routing: sequenced, machine-assigned, scheduled
    materials:  Vec<MaterialReservation>,   // bill of materials for A7

    created_at, updated_at,
}

struct JobOperation {
    op_id: Uuid,
    seq: u32,                         // routing order — mutable (engine order ≠ production order)
    kind: JobOpKind,                  // Print | Finishing | Prepress | Cutting | …
    component_role: Option<String>,   // set for Print steps
    operation: Option<record<operation>>,   // set for Finishing steps
    suggested_machine: Option<record<machine>>,  // engine's pricing pick (reference)
    planned_machine:   Option<record<machine>>,  // production's assignment; seeded = suggested
    planned_sheets:    Option<u32>,              // seeded from breakdown; editable after re-imposition
    status: JobOpStatus,              // queued → on_machine → done
    scheduled_start: Option<datetime>,
    scheduled_end:   Option<datetime>,
}

struct MaterialReservation {
    material: record<material>,
    source: String,                   // provenance: "component:interior" / "operation:lamination"
    basis: MaterialBasis,             // per_sheet | per_m2 | per_cm | per_item
    planned_qty: i64,                 // seeded from breakdown (waste-inclusive → a purchasing estimate)
    reserved_qty: i64,                // A7 fills on release
    consumed_qty: i64,                // A7 fills on completion; planned ≠ consumed feeds A8 + waste tuning
}
```

## Derivation at confirm (pure function of the frozen snapshots)

For each engine-priced quote line on the order, build one `ProductionJob`:

1. Copy `job_spec`, `quoted_breakdown`, `pricelist_version` verbatim into the
   frozen half.
2. Seed `operations`:
   - one `Print` `JobOperation` per **printed** component, with
     `planned_machine = suggested_machine = breakdown.machine_id` and
     `planned_sheets = breakdown.sheets`;
   - one `Finishing` `JobOperation` per `JobSpec.operations` entry, in JobSpec
     order. `seq` is mutable because the engine's operation order is not
     guaranteed production-correct (e.g. laminate-before-cut) — production
     resequences.
3. Seed `materials`:
   - one `MaterialReservation` per component — **including unprinted ones**
     (e.g. a board backing: the breakdown carries it with `machine_id: None`),
     `planned_qty` from the component's `sheets`;
   - one per operation that has a `material` param, `planned_qty` from the
     operation's unit-basis figure.

Manual lines carry no engine plan and produce no job — by design. An order
whose only lines are manual has zero jobs, which is allowed.

## Material planning (A7)

The per-job `materials` list is the bill of materials. A7 walks it:

- **On job release** — reserve `planned_qty` against `material` stock levels.
- **On operation / job completion** — record `consumed_qty`.
- **Reorder alerts** when stock falls below a threshold.

`planned_qty` is seeded from the breakdown's sheet/area figures, which include
the price model's conservative waste (`waste_percent` + `waste_fixed_sheets`),
so it is a **purchasing estimate, not exact consumption**. Keeping `planned`,
`reserved`, and `consumed` separate lets A7 report the difference and lets the
tenant tune the waste model against reality.

## Margin (A8)

`quoted_breakdown` (frozen) is the as-quoted cost baseline; the accepted quote
line's `final_total_minor` is the price. Actual production cost —
`consumed_qty × current material price` plus real machine time — is compared
against the baseline per job. This is exactly the "engine cost vs invoiced
price" comparison A8 promises, now anchored on a stable per-job snapshot.

## Deviations from `staff-quoting.md`

`staff-quoting.md` §"Quote → order" originally kept the `job_spec` +
`breakdown` snapshots on the quote and had production *follow the
`order.quote` link*. This design instead **snapshots them onto the
`production_job`** at confirm. Rationale (quote mutability, hot-path reads,
A8 baseline) and the full trade-off are in
`docs/adr/0015-production-plan-snapshots-onto-job.md`. `staff-quoting.md`'s
conversion section is patched to match; the `quote` ↔ `order` document link and
the `quote_line_ref` on the job remain for traceability.

## Resolved sub-decisions

- **Job creation timing:** at `Confirmed` (the board can queue immediately),
  not lazily at `InProduction`.
- **Order status:** a rollup of job progress with a staff override, not the
  prototype's manually flipped flag.
- **Manual-only orders:** allowed to have zero jobs.

## What to build when

- **Step 3 (quote→order, now):** add the stable `line_id` to embedded quote
  lines and the `source: Option<QuoteLineRef>` on order lines. This is the only
  change needed up front and it unblocks everything later. Do **not** build the
  `production_job` entity yet.
- **A6:** the `production_job` entity, confirm-time derivation, the job board,
  and the `OrderStatus` rollup.
- **A7:** `MaterialReservation` reserve/consume against stock and reorder
  alerts.
- **A8:** the per-job as-quoted vs. as-built margin comparison.
