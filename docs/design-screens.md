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

| Screen | Flag | Description |
|---|---|---|
| Quote list | 🟢 | Table: number, customer/prospect, status badge, total, linked order. |
| Quote detail / editor | 🔴 | Main screen. Header (customer-or-prospect, currency, status badge with lifecycle actions: send/accept/decline, reprice, clone, convert-to-order), a **line list** mixing all three tier types, running totals, notes, valid-until. Needs a clear layout for heterogeneous line rows. |
| Line composer — three add modes | 🔴 | Mode switcher opening one of: **Product (tier 1)** — template picker → portal parameter controls → live ladder + breakdown *(depends on Step 4, lands later)*; **Expert (tier 2)** — see below; **Manual (tier 3)** — description, qty, unit price (trivial row). |
| Tier-2 expert composer | 🔴 | **The hardest screen.** A component grid (role, pages, colors like "4/0", material picker, optional machine pin), an operations list (operation picker + params), format + quantity inputs, and a **live breakdown panel** that recomputes on change (debounced `POST /api/estimate`) showing per-component machine choice, sheet counts, and cost. Same interaction rhythm as the portal configurator, expert controls. |
| Adjustment panel | 🔴 | Per engine-priced line, visible only with `quotes:override`: engine price struck-through next to final price, a mode toggle (margin override / discount % / price override), and a reason field. |
| Quote→order confirmation | 🟡 | Modal/step; for a prospect-quote it must first offer "create customer" inline before converting. |

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
