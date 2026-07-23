# Design: Staff Quoting — internal estimating and quote documents

## Status

Proposed — not yet implemented. Elevates the one-line mention in PLAN.md
A4 ("internal estimating uses the same engine with staff-only overrides")
into a first-class feature, designed so it can ship **before** the
customer portal (A3) and before the template authoring UI (A2). See
"Sequencing" at the end for what that reordering looks like.

Related docs: `docs/instant-quote.md` (portal narrative),
`docs/product-configuration.md` (template/effects layer),
`docs/quote-engine-spec.md` (normative engine spec — this design adds a
small set of deltas to it, listed in §"Spec deltas"),
`docs/rbac-design.md` (permission catalog this design extends),
`docs/order-production-design.md` (target Order + `production_job` model that
consumes the quote→order output; supersedes the conversion snapshot rule
below — see `docs/adr/0015-production-plan-snapshots-onto-job.md`).

## Context

The existing quoting docs were written portal-first: the template/effects
layer exists to let an anonymous customer produce a priceable job from
dropdowns, and the staff side appears only as a footnote (the
"authenticated back-office variant" of the quote endpoint returning
`breakdown`, quote-engine-spec §7).

Staff quoting has different requirements:

- A manager must be able to estimate jobs **outside** the standard
  customer-facing templates — arbitrary component combinations, any
  catalog material or operation, off-ladder quantities, one-off jobs.
- Quotes are **documents** with a lifecycle (drafted, sent to the
  customer, accepted → order), not anonymous price lookups.
- Staff need **commercial control**: margin overrides, discounts,
  manual prices — with an audit trail of who deviated from the engine
  price and by how much.

The architecture already separates cleanly into two layers, and the
whole design falls out of that split:

1. **Engine core** — `PriceModel + JobSpec → breakdown/price`
   (quote-engine-spec §2, §6). Pure cost math over physical facts;
   knows nothing about templates.
2. **Template/effects layer** — the customer-facing *authoring*
   mechanism that turns dropdown selections into a `JobSpec` (§3–§5).

Everything that would limit a staff user lives in layer 2. So: **same
engine, different front door** — staff get entry points that skip or
supplement the template layer, never a second pricing implementation.
One engine means the quoted price, the order's production plan, and the
margin report (A8) always agree.

## Goals / non-goals

**Goals**

- Staff can price any job expressible in the tenant's catalog
  (formats, materials, machines, operations) without a product template
  existing for it.
- Staff can produce a quote document for a customer, track its
  lifecycle, and convert an accepted quote into an order.
- Commercial overrides (margin, discount, manual price) are possible,
  permission-gated, and audited: the engine price and the final price
  are both stored.
- Jobs the engine cannot cost (parametric packaging, per-m² roll work —
  the engine-boundary items in `product-configuration.md`) still get a
  quote via manual lines. The engine is a calculator, never a gate.
- Shippable before A2/A3: tier 2 + tier 3 (below) depend only on the
  engine core and seeded pricing data, not on templates.

**Non-goals (v1)**

- Quote revisions/versioning — a sent quote is immutable; revise by
  cloning (see lifecycle).
- Quote PDF + email delivery — that's A5; v1 ends at status `sent`
  (staff deliver the quote out-of-band).
- Extending the engine vocabulary (parametric geometry, per-m²
  technology model, new unit bases) — staff mode does not unlock those;
  they remain engine releases. Manual lines are the escape hatch.
- Customer-visible quote acceptance (portal "accept" button) — arrives
  with A3/A4; v1 acceptance is recorded by staff.

## The three tiers

A quote line is priced in one of three ways, mixable within one quote:

### Tier 1 — from a product template

The same resolution path as the portal (selection → effects → JobSpec →
price), plus what the portal never gets: the full cost `breakdown`
(per-component machine choice, sheet counts, `cost_micro` per
component/operation) and the commercial override panel. Fast path for
standard products once templates exist. **Requires A2-era template
data**, so in a staff-first build order this tier lands last.

Compatibility rules (§5) still apply — they encode physical constraints
("spiral binding needs ≥ 20 pages"), not marketing. Option
`available: false` flags also apply: they mean "we temporarily can't
fulfill this", which is as true for a staff order as a portal one.
Staff see the violation message and can fall back to tier 2 if they
know better; they don't get a bypass toggle on tier 1.

### Tier 2 — expert estimate (direct JobSpec)

Staff compose a `JobSpec` directly: quantity, format, components (role,
pages, colors, material), operations with params, optional technology
constraint. No template, no effects, no compatibility rules, no
quantity ladder — any quantities the user asks for.

Validation is the engine's own safety net, nothing more: the
completeness check (E106) plus pricing errors (E201 no capable machine,
E202 item larger than sheet, E204 material basis mismatch, E205 invalid
reserved param). Resolution errors E101–E110 don't apply — there is no
resolution. This stops staff from quoting a job the shop physically
cannot run while allowing every job it can.

This tier is what makes staff quoting buildable first: it needs the
`PriceModel` tables (§2), the pricing math (§6), and catalog data —
not the effects/resolution machinery (§3–§5) and not the template
editor.

### Tier 3 — manual lines

Free-form line items: description, quantity, unit price. For jobs
beyond the engine's vocabulary, resale items, or "the customer supplies
the paper" one-offs. No engine involvement, no breakdown — and the
margin report later shows them as cost-unknown, which is honest.

## Quote entity

No quote table exists today (survey: `customer`, `order`, `invoice`
only). New per-tenant table, following the `order` conventions
(prefixed number from the atomic `counter`, tenant-scoped repo, live
updates):

```
quote: {
  id, number,                       // counter-assigned, tenant quote_prefix
  customer: option<record<customer>>,
  prospect: option<{ name, email?, phone? }>,   // quoting before the customer exists
  currency,                          // tenant default, same resolution as orders
  status,                            // lifecycle below
  valid_until: option<datetime>,
  lines: [QuoteLine],                // embedded, like order line items
  pricelist_version: option<int>,    // set iff ≥1 engine-priced line
  notes,
  created_by,                        // user_id from AuthContext
  created_at, updated_at
}
```

Exactly one of `customer` / `prospect` must be set. Converting an
accepted prospect-quote to an order requires creating the customer
first (orders require `customer_id`); the UI offers that as one step.

### QuoteLine

Tagged union mirroring the three tiers. Every variant carries a stable
`line_id` (uuid, assigned on create, preserved across edits and clone) so an
order line and its `production_job` can reference the exact source line — see
`docs/order-production-design.md`:

```
QuoteLine =
  | Template { template: record<product_template>,
               selection: map, 
               qty: u32,  // >= 1
               pricing: EnginePricing }
  | Spec     { job_spec: JobSpec, 
                description: string, 
                qty: u32,  // >= 1
               pricing: EnginePricing }        // qty duplicated from job_spec.quantity for uniform reads
  | Manual   { description: string, 
              qty: u32,  // >= 1
              unit_minor: i64 }

EnginePricing = {
  breakdown: Breakdown,              // snapshot: component/operation rows, machines, sheets, cost_micro
  engine_total_minor: i64,           // what §6.4 produced with the policy's own band
  adjustment: option<Adjustment>,
  final_total_minor: i64             // engine_total_minor with adjustment applied
}

Adjustment =                          // at most one per line, v1
  | MarginOverride { multiplier_bp: u32 }      // must be > 0; replaces the policy band multiplier
  | Discount       { percent_bp: u32 }         // 0..=10_000; off engine_total_minor
  | PriceOverride  { total_minor: i64 }        // must be >= 0; manual final price, engine price kept
-- each with { reason: option<string> }
```

The stored `breakdown` + `pricelist_version` make every quote
reproducible and auditable ("priced under version 42, engine said
€290.30, manager overrode to €275, reason: key account"). While the
quote is a draft, a **re-price** action re-runs the engine against the
current price model and updates snapshots (flagging lines whose price
changed); after `sent`, nothing is recomputed.

The engine-cost-vs-final-price pair per line is precisely what the A8
margin report consumes later.

### Lifecycle

```
draft ──► sent ──► accepted ──► (order created)
  │          │          │
  │          ├──► declined
  │          └──► expired ◄──────┘  (valid_until passed)
  └──► (deleted — drafts only)
```

- `draft` — freely editable, re-priceable.
- `sent` — immutable content; only status transitions allowed. To
  revise, **clone to a new draft** (new number, `revises:
  option<record<quote>>` back-link) — same immutability philosophy as
  invoices.
- `accepted` — records acceptance (v1: staff clicks it, notes how the
  customer accepted). Conversion to order is a separate explicit action,
  so a tenant can accept without converting yet.
- Reads/lists may mark overdue `sent` or `accepted` quotes as `expired`
  lazily. More importantly, acceptance and order conversion each
  re-evaluate `valid_until` against database time inside their transaction.
  If it has passed, the transaction marks the quote `expired` and commits
  without accepting it or creating an order; the endpoint then returns
  409. This check and the requested action are atomic, so a concurrent
  request cannot accept or convert an expired quote.
- Invalid transitions → 409, same `validate_transition` pattern as
  `OrderStatus`.

### Quote → order

`POST /api/quotes/{id}/order` (quote must be `accepted`, unexpired, and not
already converted; the transactional expiry check above always runs):

- Creates a draft `Order` for the quote's customer. Manual quote lines
  become one order `LineItem` with their quoted quantity and unit price.
  For an engine-priced line with quantity `q`, divide `final_total_minor`
  as `q * base_minor + remainder`, where `0 <= remainder < q`: create a
  line of quantity `q - remainder` at `base_minor` per unit and, when the
  remainder is non-zero, a second line with the same description of
  quantity `remainder` at `base_minor + 1`. This allocates each residual
  minor unit without changing the accepted price. Template/spec lines
  synthesize their description from the product name + key parameters.
  The resulting order total must equal the sum of the accepted quote's
  manual line totals and engine-priced `final_total_minor` values exactly;
  conversion fails rather than creating an order if that invariant does
  not hold.
- Order gains `quote: option<record<quote>>`; quote gains `order:
  option<record<order>>`. One order per quote (v1). Each order line also
  carries `source: option<{ quote, line_id }>` pointing at the quote line it
  was priced from (`None` for manual lines); the two order lines produced by
  the residual-minor split share one `line_id`.
- The engine-priced lines' `job_spec` + `breakdown` are the production plan.
  A6 does **not** read them live off the quote: at Order confirm they are
  snapshotted onto a dedicated `production_job` (one per engine-priced quote
  line), which owns the mutable shop-floor plan production executes. The quote
  keeps its own copies as the immutable commercial record; the job's
  `quote_line_ref` links back for traceability. Full model and rationale:
  `docs/order-production-design.md` and
  `docs/adr/0015-production-plan-snapshots-onto-job.md`.

## Spec deltas (quote-engine-spec.md)

Staff mode needs four small, precise additions to the normative spec —
all in the engine's spirit, none touching existing formulas:

1. **Normative `JobSpec` wire format** (§4 gains a subsection). The
   shape already appears as the §9.3 fixture; promote it to a defined
   request schema so tier 2 can submit it: `format` (record id),
   `quantity`, `components[] {role, pages, colors, material}`,
   `operations[] {operation, params}`, `technology_allow`. Direct-spec
   validation = referenced-id existence (`E107`) + completeness
   (`E106`) + reserved-param checks; **not** E101–E105 (those are
   resolution errors).
2. **Per-component machine pin** — optional `machine: Option<MachineId>`
   on a submitted component: restrict the §6.2 capable set to that
   machine (capability checks still apply; not capable → `E201`).
   Covers "I know this runs on the offset press" without disturbing
   auto-pick for everyone else. Not expressible from templates (no
   effect sets it) — deliberately staff-only.
3. **Margin override input** — `quote(model, spec, qty, margin_override:
   Option<u32 /* bp */>)`: when present, replaces the §6.4 band
   multiplier. Rounding and `min_price_minor` still apply. Discounts and
   price overrides are *not* engine inputs — they're applied to the
   engine result by the API layer and stored in `Adjustment` (keeps the
   engine deterministic and the audit trail in one place).
4. **Breakdown response schema** — §7 already promises the back-office
   `breakdown`; define its shape normatively (component rows: role,
   machine_id, sheets, cost_micro; operation rows: operation, cost_micro;
   totals) since it's now stored on quote lines, not just displayed.

## API

Stateless pricing (no persistence — the calculator):

| Endpoint | Body | Returns |
|---|---|---|
| `POST /api/estimate` | `{ job_spec, quantities: [u32], margin_override_bp? }` | per-qty `{ total_minor, unit_minor, breakdown }` |
| `POST /api/estimate/template` | `{ template, selection, quantities?, margin_override_bp? }` | same, plus resolved job_spec echoed (so the UI can open it in the tier-2 composer — "start from template, then tweak the spec") |

Quote documents:

| Endpoint | Purpose |
|---|---|
| `GET /api/quotes`, `POST /api/quotes` | list / create (draft) |
| `GET|PUT|DELETE /api/quotes/{id}` | read / update (draft only) / delete (draft only) |
| `POST /api/quotes/{id}/status` | lifecycle transitions |
| `POST /api/quotes/{id}/reprice` | draft only: re-run engine lines against current price model |
| `POST /api/quotes/{id}/clone` | new draft from any quote (the revision mechanism) |
| `POST /api/quotes/{id}/order` | convert accepted quote → draft order |

All authenticated + tenant-scoped via the existing `require_auth` /
`repo_for` path. `PUT` recomputes engine pricing for changed lines
server-side — the client never submits prices for engine lines, only
specs/selections and adjustments.

## RBAC

Extends the `rbac-design.md` catalog (new resource, default
assignment):

| Permission | admin | manager | sales | production | finance |
|---|:-:|:-:|:-:|:-:|:-:|
| `quotes:read` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `quotes:write` (create/edit/send/accept/convert) | ✓ | ✓ | ✓ | | |
| `quotes:override` (any `Adjustment`; margin_override on estimates) | ✓ | ✓ | | | |

Sales can quote at list price all day; deviating from the engine price
takes a manager. `POST /api/estimate*` requires `quotes:read` (it
exposes cost internals — acceptable for v1 across all roles; tighten to
a dedicated `estimates:read` later if cost visibility for production/
finance turns out to be sensitive).

## Frontend

New `features/quotes/` following the existing feature-folder pattern
(`api.ts`, `types.ts`, List/Detail/Form), routes `/quotes`,
`/quotes/new`, `/quotes/$id`:

- **Quote list/detail** — the standard TanStack Table + detail pattern;
  status badges, customer/prospect, totals, linked order.
- **Line composer**, three add-line modes matching the tiers:
  - *Product* (tier 1): template picker → the same parameter controls
    the portal will later use → live ladder + breakdown.
  - *Expert* (tier 2): the centerpiece. Component grid (role, pages,
    colors, material picker, optional machine pin), operations list
    (operation picker + params), format + quantity. Debounced
    `POST /api/estimate` on change; breakdown panel updates live —
    the same interaction rhythm as the portal configurator, different
    controls.
  - *Manual* (tier 3): description, qty, unit price.
- **Adjustment panel** per engine line (visible with `quotes:override`):
  engine price shown struck-through next to the final price, reason
  field.
- Quotes join the omnibox search and the WS live-update stream like
  other entities (see the live-entity onboarding steps).

## Sequencing — building staff quoting first

The staff-first build order and what each step actually requires:

| Step | Scope | Needs |
|---|---|---|
| 1 | Engine core: §1 money, §2 `PriceModel`, §6 pricing (+ deltas 1–4) | nothing — pure crate, golden-fixture-testable from day one |
| 2 | Minimal catalog admin: CRUD for `format`, `material`, `machine`, `operation`, `pricing_policy` (+ `pricelist_version` stamping, snapshot rebuild) | step 1. This is the small slice of A2 — no template editor, no linter. Task-level breakdown: `docs/pricing-admin-plan.md` phase A2a |
| 3 | Quote entity + API + tier 2/3 UI | steps 1–2, RBAC (B1) for the permission gates |
| 4 | Templates: §3–§5 (effects, resolution, rules) + template editor + lint → tier 1 | steps 1–3 |
| 5 | Portal (A3) | step 4 — and by now the engine, catalog, rules and even the parameter-rendering UI are battle-tested by staff use |

Step 2 is the honest cost of going staff-first: the engine is useless
without catalog data, so the materials/machines/operations screens
can't be deferred — only the template editor can. The payoff: real
tenant staff exercise the engine and price model on real jobs long
before anything is exposed publicly, and the portal becomes a thin new
front door on proven machinery.

PLAN.md impact: A4's estimating clause is superseded by this doc; the
suggested sequence's step 3 becomes "A2 catalog slice → staff quoting →
A2 template editor → A3 portal → A4 pipeline", and quote→order
conversion (here) subsumes half of A4 (the other half — portal quote
*requests* from anonymous customers — stays with A3/A4).

## Known gaps, deliberately deferred

- **One-off material pricing** ("customer supplies the paper", spot-buy
  stock): v1 answer is a manual line or a scratch catalog row. A
  per-line material price override is a clean later addition to the
  spec (same shape as the machine pin) once the need is proven.
- **Waste override** for a known-clean job: not in v1; the waste model
  is per machine and quotes err conservative.
- **Per-component operation scoping**: v1 operations are job-level — they
  bind to a whole-job dimension by `unit_basis` (`per_item`/`per_m2` → finished
  items, `per_sheet` → the sum of *all* components' sheets, `per_cm` → the job
  format edge). Qty- and area-based finishing already scopes correctly to
  finished items, so the gap is specifically a **`per_sheet` operation that
  should apply to a subset of components** (e.g. sheet-level coating or spot-UV
  on the cover's sheets only) — it over-bills against total job sheets and
  cannot be targeted. v1 answer is a manual line for that charge. The clean
  later addition is an optional operation `target` scope (a set of component
  roles), the same shape as the per-component machine pin — an additive engine
  delta, no re-architecture. Flexibility here is core to the expert composer's
  purpose, so this is a "when proven", not "never".
- **Partial-quantity finishing**: applying an operation to only part of the
  order quantity ("laminate 200 of the 500") is not expressible in v1 — an
  operation's cost keys off the whole `job.quantity`. Real but occasional; v1
  answer is a manual line (or splitting into two quote lines). A later
  per-operation quantity/fraction input is a bounded engine delta once the need
  recurs.
- **Staged / post-assembly operations with quantity transformation** — the big
  one, and a model evolution rather than an additive delta. v1 is single-stage
  and flat: operations are an unordered set, each billed against a *static*
  job-level dimension (`qty`, `total_sheets`, format edge/area) computed from the
  original components. There is no assembly point (components → one unit) and no
  working count that flows and transforms between operations — so an operation
  that changes the piece count (each leaf cut into two or more, n-up-then-
  guillotine, fold, gang) cannot make a downstream operation count the new
  number. A complete production model needs operations to become an **ordered
  pipeline** threading a working piece-count, with an assembly stage and
  per-stage quantity transforms. The production layer already carries half of
  this — `order-production-design.md`'s `JobOperation` is *sequenced* and notes
  the engine's operation order "is not guaranteed production-correct (e.g.
  laminate-before-cut)" — but only for scheduling/execution, not for cost math.
  This is a future engine version (JDF-style process chain), the likely point
  where the quote engine and the production routing model converge; not v1, and
  not a drop-in field. v1 answer remains manual lines / split lines for anything
  the flat model mis-costs.
- **Parametric geometry / per-m² roll printing / new unit bases**: the
  engine-boundary items from `product-configuration.md` remain engine
  releases; staff mode changes nothing about them. Manual lines carry
  those quotes until the vocabulary grows.
- **Quote PDF and email** (A5), **customer-side acceptance** (A3/A4),
  **revision chains beyond clone-with-back-link** — all explicitly
  after v1.

## Testing

- Engine deltas: machine pin (capable/not-capable/absent), margin
  override replaces the band exactly (golden-fixture variant),
  direct-spec validation matrix (E106/E107/E204/E205 fire; E101–E105
  cannot).
- API: quote CRUD + lifecycle transition table (every status × every
  action), sent-quote immutability, clone back-link, convert — happy
  path, double-convert 409, prospect-quote convert requires customer;
  route × role sweep extension for the three new permissions;
  adjustment without `quotes:override` → 403.
- Repricing: model change between draft and reprice updates snapshots
  and flags changed lines; `pricelist_version` recorded per §2.
- Property: for any valid direct spec, quote(qty) with no adjustment
  equals the tier-1 price of a template resolving to the same spec —
  one engine, byte-identical (the "same front door" invariant).
