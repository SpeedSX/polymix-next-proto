# Quote Engine & Portal — Implementation Sequence

Build order for Track A (A1–A4) of the post-prototype roadmap, staff-first.
Short and actionable; the **normative** contracts live elsewhere and win on any
conflict:

- `docs/quote-engine-spec.md` — engine math, schemas, golden fixture (§9). Normative.
- `docs/staff-quoting.md` — staff estimating, quote documents, spec deltas 1–4. Normative for the staff layer.
- `docs/product-configuration.md` — effects/template narrative.
- `docs/instant-quote.md` — portal narrative.
- `docs/pricing-admin-plan.md` — catalog + template admin task breakdown.
- `docs/rbac-design.md` — permission catalog (B1).

Rule: work one step at a time, in order. A step is done only when its **Done
when** list passes and `just check` is green. Record deviations as an ADR under
`docs/adr/`.

---

## Step 1 — Engine core (`crates/quote-engine`)

Pure, deterministic crate. No I/O, no async. Implements `quote-engine-spec.md`
§1–§6 plus `staff-quoting.md` spec deltas 1–4 (normative `JobSpec` wire format,
per-component machine pin, margin-override input, breakdown response schema).

Module layout per the spec header: `money.rs §1 · model.rs §2 · effect.rs §3 ·
resolve.rs §4 · rules.rs §5 · price.rs §6 · fixtures/ §9`.

**Depends on:** nothing. Can start pre-gate.

**Done when:**
- Golden fixture (`fixtures/demo.json` + `expected.json`) reproduces §9.4/§9.5
  byte-identically, including per-component/operation `cost_micro`.
- All §10 required tests pass: ups table, E101–E110, rule eval, selection
  validation, ladder monotonicity, determinism, no-float grep, units_multiplier.
- Spec deltas covered: machine pin (capable/not/absent), margin override
  replaces the band exactly, direct-spec validation matrix (E106/E107/E204/E205
  fire; E101–E105 cannot).

## Step 2 — Catalog admin (A2 slice)

Backend CRUD + minimal Mantine screens for `format`, `material`, `machine`,
`operation`, `pricing_policy`. `pricelist_version` (`meta:pricing`) incremented
in the same transaction on every mutation. In-memory `Arc<PriceModel>` snapshot,
rebuilt on a SurrealDB live query over the pricing tables (requests never touch
the DB). No template editor, no linter yet.

Task-level breakdown: `docs/pricing-admin-plan.md` phase A2a.

**Depends on:** Step 1.

**Done when:**
- All five pricing tables have working CRUD from the UI, tenant-scoped.
- A rate edit bumps `pricelist_version` and rebuilds the snapshot without a restart.
- The seeded demo price model (§9.1 dataset) loads and prices the golden request.

## Step 3 — Quote documents + staff estimating (tiers 2 & 3)

`quote` entity/table per `staff-quoting.md` (counter-numbered, tenant-scoped,
embedded lines, lifecycle). Stateless pricing endpoints `POST /api/estimate` and
`POST /api/estimate/template`. Quote CRUD + lifecycle + `reprice` + `clone` +
quote→order conversion. Frontend `features/quotes/` with the tier-2 expert
composer (component grid, operations, machine pin) and tier-3 manual lines.

**Depends on:** Steps 1–2, **B1 RBAC** (permission gates `quotes:read/write/override`).

**Done when:**
- Staff price an arbitrary in-catalog job (no template) via tier 2 and see the
  live breakdown; manual lines (tier 3) work.
- Lifecycle transition table enforced (draft→sent→accepted→order; declined/
  expired); sent quotes immutable; clone carries the `revises` back-link.
- Quote→order conversion happy path + double-convert 409 + prospect-quote
  requires customer first.
- Adjustment without `quotes:override` → 403; engine price and final price both stored.
- Property test: a valid direct spec prices byte-identically to a template
  resolving to the same spec (the "same front door" invariant).

## Step 4 — Templates + editor + lint (unlocks tier 1)

Effects/resolution/rules machinery (`quote-engine-spec.md` §3–§5) is already in
the crate from Step 1; this step adds the tenant-facing template editor and the
save-time linter (§8), and wires tier-1 (from-template) quote lines.

**Depends on:** Steps 1–3.

**Done when:**
- Template editor round-trips a `product_template` (parameters, options, effect
  builder, compatibility rules) atomically; publish stamps `pricelist_version`.
- Lint blocks save on §8 errors (incomplete default, unavailable default,
  per-option resolve/price failure, dangling id) and surfaces warnings.
- Tier-1 quote lines resolve selection → JobSpec → price with the full breakdown.

## Step 5 — Public portal (A3)

Public, unauthenticated route group; tenant resolved by subdomain/slug.
`GET /portal/products/:slug/schema` and `POST /portal/products/:slug/quote` per
`quote-engine-spec.md §7`. Configurator UI with live price ladder, greyed-out
incompatible/unavailable options, debounced re-quote, custom quantity. Public
response **never** includes `breakdown`. Rate-limited (anonymous).

**Depends on:** Step 4.

**Done when:**
- Configurator renders a template, greys options via availability + compatibility
  rules, and shows the ladder updating <300 ms per change (server-computed).
- Quote submission stores the quote with its `pricelist_version`.
- Portal response omits cost internals; API enforces option `available: false`
  regardless of UI state.

---

## Dependencies & sequencing notes

- **B1 RBAC gates Step 3.** Either land B1 first or ship Step 3 behind a stubbed
  permission layer and record the deviation (see PLAN.md §"Suggested sequence").
- **Steps 1–2 are pre-gate-safe** — pure engine + seeded catalog data, no coupling
  to the SurrealDB-vs-Postgres gate verdict.
- **Quote→order** (Step 3) subsumes the estimating half of PLAN.md A4; anonymous
  portal quote *requests* stay with A3/A4 (Step 5+).
- Message/label i18n in templates and rules must use the app locales (`uk`/`en`),
  not the `pl` placeholders in the narrative docs.
