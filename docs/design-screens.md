# Quote Engine & Portal — Screens Needing Design

Screen inventory for Track A (Steps 2–5 of `docs/quote-implementation-plan.md`).
Step 1 is a pure crate — no UI.

Complexity flags:

- 🔴 **novel/composite** — no precedent in the app, needs real design.
- 🟡 **structured form** — beyond plain fields (conditional sections, repeating rows), needs layout thought.
- 🟢 **reuses existing pattern** — the customers/orders/invoices List/Detail/Form + status-badge pattern; design optional.

Context: Step 2–4 screens are **authenticated back-office** — match the current
Mantine 8 / TanStack Table app style. Step 5 is a **distinct public surface**
(no app chrome, no nav) — a customer-facing storefront look.

---

## Step 2 — Catalog admin

One nav section ("Price model" / "Catalog"). Lists are standard tables; the forms
carry the design weight because of nested/conditional structure.

| Screen | Flag | Description |
|---|---|---|
| Catalog list screens | 🟢 | Five sortable/paginated tables — Formats, Materials, Machines, Operations, Pricing policies. Standard table + "New" button. |
| Format form | 🟢 | Name + width/height (mm), portrait constraint. Trivial. |
| Material form | 🟡 | Name, `kind` (free taxonomy), and a **pricing-basis switcher**: choosing `per_sheet / per_m2 / per_cm / per_item` swaps the visible fields (per_sheet adds a sheet-size input). Optional "printable" toggle revealing a grammage field. Opaque `attrs` key/value editor. The conditional field-set is the design problem. |
| Machine form | 🟡 | Name, technology (`digital \| offset`), sheet size, duplex, max grammage, setup/waste fields, **plus technology-conditional cost fields** (digital → click_mono/click_color; offset → plate_price/run_price). Same conditional-section challenge as Material. |
| Operation form | 🟡 | Name, setup, `unit_basis` select, unit price, and a small **reserved-params** area (material picker, edge_mm, units_multiplier). |
| Pricing-policy form | 🔴 | Currency, **margin-bands editor** (add/remove/reorder rows of `min_qty → multiplier`, first band pinned at qty 1), rounding step + mode, min price. The repeating-row band editor with validation is novel. |

## Step 3 — Quote documents + staff estimating (centerpiece)

Comps live in `docs/design/PolyMix Logo.dc.html` (section ids in the **Comp** column).

| Screen | Flag | Comp | Description |
|---|---|---|---|
| Quote list | 🟢 | — | Table: number, customer/prospect, status badge, total, linked order. |
| Quote detail / editor | 🔴 | 14a · 14b | Main screen, **two modes in one route** by status: *draft* → editor (everything mutable), *sent/accepted/declined/expired* → frozen record (lifecycle actions + clone only). Header (customer-or-prospect, currency, status badge with status-dependent actions: send/accept/decline, reprice, clone, convert-to-order), a **line list** mixing all three tier types, running totals, notes, valid-until, linked-order banner once converted. Needs a clear layout for heterogeneous line rows. See decided-details below. |
| Line composer — three add modes | 🔴 | 14a (add-line bar) | Mode switcher opening one of: **Product (tier 1)** — template picker → portal parameter controls → live ladder + breakdown *(depends on Step 4; gate as "coming soon" until then)*; **Expert (tier 2)** — see below; **Manual (tier 3)** — description, qty, unit price (trivial row). |
| Tier-2 expert composer | 🔴 | 13a · 13b (states) · 13c (mobile) | **The hardest screen.** A component grid (role, pages, colors like "4/0", material picker, optional machine pin), an operations list (operation picker + params), format + quantity inputs, and a **live breakdown panel** that recomputes on change (debounced `POST /api/estimate`) showing per-component machine choice, sheet counts, and cost. Includes the **quantity ladder** (ad-hoc qty chips → per-qty total/unit): cheap because the engine's `quote_spec` already prices a `quantities[]` list; composer-time aid only, the saved line stores the **primary** quantity's pricing. Same interaction rhythm as the portal configurator, expert controls. |
| Adjustment panel | 🔴 | 15a | Per engine-priced line, visible only with `quotes:override`: engine price struck-through next to final price, a mode toggle (margin override / discount % / price override), and a reason field. Margin override re-runs the engine on save (rounding + min price); discount and price override are API-layer, applied to the engine result. |
| Quote→order confirmation | 🟡 | — | Modal/step; for a prospect-quote it must first offer "create customer" inline before converting. |

### Quote detail / editor — decided details

Layout regions, top to bottom: page header (breadcrumb · number · status badge · status-dependent action cluster) → party & metadata band (customer-or-prospect toggle, currency read-only, valid-until, `pricelist_version` chip, created-by footnote) → the heterogeneous **line list** → totals footer (sum of `final_total_minor` + manual totals, authoritative — the quote→order split depends on it) → notes → linked-order banner (record mode).

Line rows share a spine (line no · type badge Product/Expert/Manual · description · qty · unit · line total · draft row-actions). Engine lines (Expert/Spec, and Product/Template once Step 4 lands) show the stored `Breakdown` snapshot; an `Adjustment` renders the engine price struck-through next to the final price with a reason indicator; a reprice-changed line carries a "price changed" flag.

Locked decisions:

- **Breakdown display = inline expand/collapse per engine line** (collapsed by default), not a side panel — the editor is a document; the two-pane rhythm stays with the tier-2 composer.
- **Draft editing = per-line inline editing**, with party/notes/valid-until always editable — not a whole-quote edit toggle. Per-line edit is what the composer and adjustment-panel screens assume.

RBAC is relaxed for now (dev-mode: all actions visible), mirroring the pricing-setup approach; the `quotes:write` / `quotes:override` gates are re-attached when B1 lands. The adjustment presentation here is the read-only reflection of the separate `quotes:override`-gated Adjustment panel.

### Adjustment panel — decided details

Per-line commercial override on **engine-priced lines only** (Template/Spec — manual lines have no engine price to deviate from). **At most one adjustment per line** (the three modes are mutually exclusive). Its purpose is the audit trail: both `engine_total_minor` and `final_total_minor` are stored and feed the later margin report. Gated on `quotes:override` (relaxed to always-visible in dev, like pricing-setup); a user without the permission sees the line at list price with no adjust affordance.

**Placement:** at the bottom of the line's inline-expand drawer — cost (breakdown) then commercial (adjustment) in one expanded region. The collapsed row spine still reflects an active adjustment (struck-through engine price + emphasized final).

Mode toggle reveals one input; note two are client-side arithmetic, one re-runs the engine:

| Mode | Input | Unit / range | Final price derivation |
|---|---|---|---|
| Margin override | multiplier, prefilled with the line's current band multiplier | `multiplier_bp` > 0 (`17000` = ×1.70) | **Re-runs the engine** — replaces the §6.4 band multiplier; rounding + `min_price_minor` still apply → needs a server round-trip, so its final figure shows a recompute state |
| Discount | percent | `percent_bp`, 0…100 % (`0..=10_000`) | Client-side off `engine_total_minor`; updates live |
| Price override | money, quote currency | `total_minor` ≥ 0 | Entered value *is* the final; engine price kept for the record; updates live |

- Margin renders as a **multiplier** (×1.70), matching the pricing-policy band editor (`multiplierToBp`/`bpToMultiplier`); show the baseline being deviated from as context (e.g. "engine margin ×1.70 (band ≥ 250)").
- **Reason** (`option<string>`) per adjustment — optional in the model, but the panel strongly encourages it (soft prompt on empty save, not a hard block).

States: no-adjustment default (final == engine, subtle "Adjust price"); editing (mode + input + reason, live final for discount/override, recompute indicator for margin); active (struck-through engine + emphasized final + kind badge + reason + "Remove adjustment" to revert); inline validation (margin ≤ 0, discount out of range, negative override); record mode (all read-only, frozen with quote content).

### Comp corrections (against the first pass in `PolyMix Logo.dc.html`)

Punch-list from the fidelity review; comp ids reference the section markers. Blocking = contradicts a locked decision or the engine model; polish = clarity/consistency.

| # | Comp | Severity | Correction |
|---|---|---|---|
| 1 | 13a | resolved — **keep** | **Quantity ladder stays and is implemented.** Cheap (engine `quote_spec` already prices a `quantities[]` list); composer-time aid only, no data-model impact — the saved line stores the primary quantity's pricing. Endpoint returns per-qty totals for the whole ladder + one full `breakdown` for the focused qty; ladder seeds with the primary quantity, staff add chips ad-hoc. |
| 2 | 14a | polish | **Gate the Product (tier-1) add mode as "coming soon"** for Step 3, and drop the live Product/Template sample line — tier 1 depends on Step 4 templates. Fine to keep a Product row in the comp as the eventual target, but label it clearly as not-yet-buildable. |
| 3 | 14b | polish | **Split the two states — never show "Convert to order" and the "Converted to Order …" banner together.** One order per quote: *accepted, not converted* → Convert button, no banner; *converted* → banner + "View order", no Convert. Comp should show one per artboard. |
| 4 | 14a | polish | Add a **Delete** action to the draft header (drafts only) — an overflow/ghost item is fine. |
| 5 | 14a | polish | Quote-line **reorder is cosmetic** (embedded array, no pricing effect). Keep the "drag to reorder" affordance only if cheap; it is not required for v1. |
| 6 | 12a (Step 2) | out-of-scope | Operations table lists invented unit bases (`per_book`, `per_signature`, `per_impression`, `per_1000_sheets`, `per_bundle`). The engine's `UnitBasis` is only `per_item / per_sheet / per_cm / per_m2` (`model.rs`). Relabel to the real four; the 13a composer already uses them correctly. |

## Step 4 — Template editor + lint

| Screen | Flag | Description |
|---|---|---|
| Template list | 🟢 | Table of product templates (name, slug, status) with a "Clone" action (tenants tweak, don't author from scratch). |
| Template editor | 🔴 | Large screen: template header (slug, i18n name, component roles, quantity ladder, custom-quantity range), a **parameter list with drag-ordering**, per parameter its options list. Select vs numeric parameter kinds render differently. |
| Effect builder | 🔴 | Nested inside each option: a repeating list of effects, each an effect-kind dropdown (fixed vocabulary) revealing type-constrained value pickers (target = component role, material picker filtered to valid materials, colors input, etc.). Most structurally complex sub-widget in the app. |
| Compatibility-rule builder | 🔴 | A recursive condition tree (all/any/not/op_present/attr) + a require clause + i18n message. Tree-shaped editor. |
| Lint results panel | 🟡 | On save: blocking errors vs warnings, each pointing at the offending parameter/option. A results surface, not a form. |

## Step 5 — Public portal (distinct visual system)

| Screen | Flag | Description |
|---|---|---|
| Product configurator (public) | 🔴 | Marquee public page. Product name/description, **parameter controls** (dropdowns/selects), options that grey out with a tooltip when incompatible or unavailable, a **live price ladder** (50→2000 + unit price, updating <300 ms per change without flicker), custom-quantity input. No breakdown, no cost internals. Storefront look, not back-office. |
| Quote request form (public) | 🟡 | After configuring: contact details (name/email/phone), artwork-upload placeholder, submit, confirmation state. |

---

## Priority guidance

The screens that make or break the feature and have **no existing precedent**:

1. **Tier-2 expert composer + live breakdown** (Step 3)
2. **Template editor / effect builder / rule builder** (Step 4)
3. **Public configurator with price ladder** (Step 5)

The 🟡 catalog forms mostly need two reusable patterns solved once — **conditional
field-sets** and **repeating-row editors** — which then cover Material, Machine,
Operation, and Pricing-policy.
