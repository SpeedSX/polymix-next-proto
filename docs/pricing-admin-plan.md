# A2 — Pricing admin (catalog + templates): explanation and work breakdown

## Status

Proposed work breakdown — nothing implemented. This decomposes roadmap
item **A2** (PLAN.md, Track A) into agent-executable tasks, restructured
into two phases per the staff-first sequencing in `docs/staff-quoting.md`:

- **Phase A2a — catalog slice.** CRUD for the five pricing tables +
  `pricelist_version` + the in-memory `PriceModel` snapshot. Blocks
  staff quoting (its step 2); contains **no** template work.
- **Phase A2b — template editor.** `product_template` +
  `compatibility_rule` authoring with the save-time linter. Blocks
  tier-1 staff quoting and the portal (A3); depends on the engine's
  resolution machinery (spec §3–§5) existing.

## Instructions for the implementing agent

Read, in this order, before writing code:

1. `docs/quote-engine-spec.md` — §2 is the **normative schema** for
   every table this feature edits; §8 is the normative linter. Do not
   invent fields; where this doc and the spec disagree, the spec wins.
2. `docs/product-configuration.md` — the narrative for what the admin
   experience should feel like ("Admin experience" section) and the
   worked templates A2b must be able to author.
3. `docs/instant-quote.md` — the snapshot/live-query architecture the
   catalog feeds ("In-memory price model").
4. `docs/staff-quoting.md` — the consumer that makes A2a urgent; its
   "Sequencing" table is the contract for what A2a must and must not
   include.

Follow the existing codebase patterns exactly:

- **Migrations**: append a `.surql` file under
  `backend/crates/surreal-store/migrations/` and register it at the end
  of the `MIGRATIONS` array in `crates/surreal-store/src/migrations.rs`
  (next free number; startup re-runs migrations across all tenant DBs).
- **Repos**: one `*_repo.rs` per entity in `surreal-store`, trait in
  `crates/domain`, tenant-scoped via the existing `repo_for` path —
  mirror `customer_repo.rs`.
- **Routes**: handlers under `crates/api/src/routes/`, registered in
  `build_router` (`crates/api/src/lib.rs`), behind `require_auth`;
  permission check as the first line of each handler
  (`auth.require(...)`, per `docs/rbac-design.md`).
- **Frontend**: one feature folder per screen group under
  `frontend/src/features/` with `api.ts`, `types.ts`, and Mantine
  components; routes in `frontend/src/app/routes.tsx`; Zod validation;
  i18n strings for `en` + `ua` (no hardcoded UI text).
- Work task-by-task in the order below; each task lists its definition
  of done. Don't start A2b before A2a is merged and the engine crate
  exposes resolve/rules (§3–§5).

Pinned decisions (don't re-open without asking):

- **Type ownership**: the `quote-engine` crate owns the `PriceModel`
  structs (spec §2) including serde; `surreal-store` and the API reuse
  them rather than defining parallel DTOs. Admin CRUD payloads are the
  stored shapes.
- **Money in the admin API/UI is micro-units end-to-end** (`i64`, spec
  §1); the UI formats for display (shows `€0.04`, stores `40000`) but
  never round-trips through floats.
- **RBAC**: new catalog permissions `pricing:read` and `pricing:write`,
  default-assigned admin ✓✓, manager ✓✓, sales ✓–, production ✓–,
  finance ✓–. Templates (A2b) reuse the same pair — templates are
  pricing data. Update the tables in `docs/rbac-design.md` and the role
  seed in the same MR that introduces the routes.
- **`pricelist_version`**: single `meta:pricing` record per tenant DB
  (spec §2); **every** mutation of any pricing table increments it in
  the same transaction. No mutation path may skip this — it is what
  makes quotes auditable.
- **Deletes are guarded**: deleting a format/material/machine/operation/
  policy that is referenced (by a template effect, another table, or —
  later — a draft quote line) returns 409 with the referencing ids.
  v1 check is a query at delete time, not FK enforcement.

## Phase A2a — catalog slice

### A2a-1 · Migration: pricing tables

New migration defining `format`, `material`, `machine`, `operation`,
`pricing_policy` tables + the `meta:pricing` version record
(initialized to 1). Schema per spec §2, schemafull where the shapes are
closed (formats, machines, policies), flexible where tagged unions live
(`material.pricing`, `attrs`).

**Done when**: migration applies cleanly on a fresh tenant DB and on
the seeded demo tenant; `cargo test --workspace` green.

### A2a-2 · Domain traits + repos

`PricingRepo` surface (per-entity CRUD + `get_version` +
`bump_version`) in `domain`, implementation in `surreal-store`.
Server-side validation on write, enforcing the spec's structural
constraints so bad data can't enter the model at all:

- format: portrait invariant `trim_mm[0] <= trim_mm[1]`, both > 0;
- material: pricing basis payload matches the tagged union; `printable`
  requires `grammage_gsm > 0`;
- machine: digital ⇒ click prices present, offset ⇒ plate/run prices
  present (and not vice versa);
- pricing_policy: bands sorted ascending, first band `min_qty == 1`,
  multipliers > 0, `rounding.mode == "up"`;
- operation: `unit_basis` in the closed set.

Delete guard per the pinned decision. Every successful mutation bumps
`meta:pricing` in the same transaction.

**Done when**: unit tests cover each validation rule (accept + reject)
and version bump on every mutation kind.

### A2a-3 · API routes

Under `/api/pricing/`: `formats`, `materials`, `machines`,
`operations`, `policies` — each `GET` (list) + `POST`, and
`GET|PUT|DELETE /{id}`; plus `GET /api/pricing/version`. Reads require
`pricing:read`, writes `pricing:write`. Validation errors → 400 with a
field-level payload the frontend can map onto form errors; delete guard
→ 409 with referencing ids.

**Done when**: integration test does full CRUD per entity against a
testcontainers SurrealDB, the route × role sweep covers the new
permission pair, and two tenants' catalogs are proven isolated.

### A2a-4 · PriceModel snapshot

Per-tenant `Arc<PriceModel>` built from the tables, cached keyed by
`(tenant_db, pricelist_version)`; the pricing endpoints (staff
`/api/estimate`, later the portal) consume the snapshot, never the DB.
On a cache miss, load the catalog and its version from one transaction
snapshot. If the store cannot provide that guarantee, read the version
before and after loading every catalog table; when they differ, discard
the model and retry, and only cache it under the stable version.
v1 invalidation: compare the cached version against `meta:pricing` per
request (one cheap point-read) — the SurrealDB live-query rebuild from
`instant-quote.md` is an optimization to add when the portal's request
volume justifies it, behind the same interface.

**Done when**: integration test — quote/estimate a fixture, mutate a
material price, estimate again: new price and new echoed
`pricelist_version`, no restart.

### A2a-5 · Seeder: demo price model

Extend `crates/seeder` to load the spec §9.1 golden dataset into the
demo tenant, so staff-quoting development and demos have a working
catalog from day one.

**Done when**: `just seed` produces a catalog on which the §9.2 request
prices to exactly the §9.6 ladder via the API.

### A2a-6 · Frontend: catalog screens

New `features/pricing/` + route `/pricing` (nav-gated on
`pricing:read`): tab or sub-route per table, standard List + Form
pattern. Specifics beyond vanilla CRUD:

- **Money inputs**: a shared `MoneyMicroInput` (display minor/decimal,
  store micro; Zod-validated integer) — build once, reuse everywhere.
- **material**: pricing-basis selector drives which payload fields
  render; `printable` toggle reveals grammage; `kind` is free text with
  suggestions from existing values; `attrs` as key/value rows.
- **machine**: technology selector drives digital-vs-offset field
  groups (mirroring A2a-2's validation).
- **pricing_policy**: margin-band table editor (add/remove/sort rows;
  client enforces first-band `min_qty` 1).
- Current `pricelist_version` visible on the section header (it's the
  audit anchor staff will quote against).
- 409 delete responses render "used by …" with links.

**Done when**: every §9.1 record can be created, edited, and deleted
through the UI against a fresh tenant; form validation mirrors server
rules; `en` + `ua` strings complete; a Form test per feature-folder
convention.

## Phase A2b — template editor + lint

Prerequisite: engine crate implements spec §3 (effects), §4
(resolution), §5 (rules), §8 (lint) — A2b is UI + persistence over
those, and must not reimplement any of their semantics client-side.

### A2b-1 · Migration + repo: `product_template`, `compatibility_rule`

Templates stored as one document with embedded parameters/options (spec
§2); rules as separate rows referencing the template. Same version-bump
and delete-guard discipline (deleting a template with live quotes/rules
→ 409; rules cascade with their template).

### A2b-2 · Template CRUD API with save-time lint

`/api/pricing/templates` CRUD; **every save runs spec §8 lint
server-side**: `errors` block the save (422 with the error list),
`warnings` are returned alongside the saved document. A separate
`POST /api/pricing/templates/lint` endpoint lints a draft without
saving, so the editor can offer "check now". Clone endpoint
(`POST /{id}/clone`) — per `product-configuration.md`, tenants tweak
copies rather than authoring from scratch.

**Done when**: an integration test per §8 lint case (1–6) proves
blocked/warned as specified; the §9.1 spiral-notebook template
round-trips byte-identically through save/load.

### A2b-3 · Frontend: template editor

The largest single UI in the milestone (`features/pricing/templates/`):

- Template list + clone; header form (slug, i18n name, component roles,
  policy picker, quantity ladder, custom-quantity bounds).
- **Parameter list** with drag-ordering (order is semantic — it is
  effect application order, spec §4.2); per parameter: select vs
  numeric kind, i18n label, options list with drag-ordering,
  `is_default` (enforced single per parameter), `available` toggle +
  unavailable message.
- **Effect builder** per option / base_effects / numeric parameter:
  effect kind from the closed §3 enum; target constrained to the
  template's component roles; value pickers constrained by kind
  (`set_material` → per-sheet-priced materials only; `add_operation` →
  operation picker + params form incl. reserved params; `set_format` →
  format picker; colors as an `F/B` input validated `0..=8`). Numeric
  parameters expose the `$input` mul/add mapping.
- **Rule builder**: v1 deliberately shallow — a rule is one optional
  `when` and one `require`, each either an `op_present` check or an
  attr comparison (path picker from §5's closed path grammar), plus a
  single-level `all`/`any` group. The full recursive AST stays
  API-expressible but is not v1 UI. i18n message editor.
- **Lint panel**: on-save errors/warnings rendered inline where
  addressable (e.g. a warning names the two parameters that overwrite
  the same attribute); "check now" against the lint endpoint.
- **Preview panel**: renders the template as the configurator will
  (parameters → controls), runs `/api/estimate/template` on change,
  shows ladder + breakdown — the admin sees what the customer and the
  tier-1 staff flow will see before publishing.

**Done when**: both worked templates from `product-configuration.md`
(spiral notebook incl. composite options, `add_component` backing, and
`$input`-less numeric alternative; newspaper `4+N` pages) can be
authored from a blank tenant through the UI, lint passing, and price
correctly in the preview.

### A2b-4 · Availability & publish flow

v1 keeps it simple: saving a lint-clean template makes it live
(version bump). No draft/published split — record as a known gap; the
`available` flags on options are the operational kill-switch.

## Explicitly out of scope for A2

- Portal endpoints/UI (A3) — A2b's preview reuses the estimate
  endpoints, not `/portal/*`.
- Matrix price overrides (`instant-quote.md` alternatives) and the
  sandboxed expression language — recorded options, not scheduled.
- Any engine-vocabulary growth (new effect kinds, unit bases, per-m²
  model).
- Machine calendars / capacity (A6), stock levels on materials (A7).

## Suggested MR slicing

One MR per task above is the default; A2a-1+2 may merge (migration is
useless without the repo), A2a-6 can land per-table if review size
demands. Every MR keeps `cargo test --workspace` and the frontend suite
green; integration tests ride with the task that makes them meaningful.
