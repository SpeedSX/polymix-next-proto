# Customers CRM profile (M5.1) — spec + build order

Extends the prototype's minimal customer entity into a CRM-grade customer
profile for the Ukrainian market: legal identification (ЄДРПОУ / РНОКПП /
ІПН ПДВ), classification & lifecycle status, multiple contact persons, and
commercial terms that feed ordering and invoicing. This document is
normative for the customer entity once M5.1 lands; where it conflicts with
PLAN.md's original `customer` block, this document wins.

Scope decisions already made (do not re-litigate):

- **No activity timeline** — interactions/calls/meetings are out of scope
  (post-prototype).
- **Contacts are an embedded array** on the customer record, edited inline
  on the customer form (same pattern as `order.line_items`). No separate
  contact entity, no sub-routes.
- All conventions from PLAN.md (ULID ids, RFC 3339 timestamps, money as
  minor units + ISO 4217, integer status codes, error envelope, layering
  handler → service → repo trait) apply unchanged.

## Data model

### customer (tenant database) — extended

```
customer {
  id: ulid,
  // No `number` field — unlike orders/invoices (which need a customer-facing
  // document reference), a customer has no external contract to number
  // against; see docs/adr/0011-drop-customer-numbering.md.
  kind: 0 | 1 | 2,                 // 0 = legal entity (ТОВ/ПП/АТ), 1 = ФОП, 2 = private individual
  name: string (required, non-empty),      // display name, e.g. "Друкарня «Аркуш»"
  legal_name: string | null,               // full legal name, e.g. ТОВ «Аркуш Прінт»
  edrpou: string | null,           // ЄДРПОУ, exactly 8 digits — kind 0 only
  tax_id: string | null,           // РНОКПП, exactly 10 digits — kind 1|2 only
  vat_ipn: string | null,          // ІПН платника ПДВ, exactly 12 digits;
                                   // null = not a VAT payer (неплатник ПДВ)
  status: 0 | 1 | 2 | 3,           // 0 lead | 1 active | 2 inactive | 3 blocked
                                   // (see /api/dictionaries/customer-statuses)
  tags: [string],                  // free-form, lowercase-trimmed by the service, unique
  industry: string | null,
  source: string | null,           // acquisition channel, free text
  website: string | null,

  contacts: [                      // 0..n, at most one is_primary = true
    {
      name: string (required, non-empty),
      role: string | null,         // "директор", "менеджер із закупівель", …
      email: string | null (format-validated),
      phone: string | null,
      is_primary: bool             // default false
    }
  ],

  legal_address: Address | null,     // юридична адреса
  delivery_address: Address | null,  // фактична адреса / доставка
                                     // Address = { street, zip, city, country } as today;
                                     // country ISO 3166-1 alpha-2, "UA" preselected in forms

  payment_terms_days: int,         // 0..365; 0 = передоплата (prepayment) — the default
  credit_limit: money | null,      // { amount_minor, currency }; null = no limit
  default_currency: string,        // ISO 4217, defaults to tenant default currency
  default_discount_bp: int,        // basis points, 0..10000, default 0

  iban: string | null,             // ^UA\d{27}$ (29 chars; МФО is folded into the IBAN)
  bank_name: string | null,

  notes: string | null,
  created_at, updated_at
}
```

Removed fields: `contact_name`, `email`, `phone`, `address` — replaced by
`contacts[]` and the two addresses (migration below). The API stops
accepting and returning them; the frontend types drop them in the same
milestone.

### Validation (domain layer, `validation_failed` 422)

Kind-conditional — the details map keys the offending field:

- `name` non-empty (unchanged).
- `edrpou`: `^\d{8}$`; **allowed only when `kind = 0`**, otherwise
  `not_applicable_for_kind`.
- `tax_id`: `^\d{10}$`; allowed only when `kind = 1 | 2`.
- `vat_ipn`: `^\d{12}$` (any kind — ФОП can be a VAT payer).
- `iban`: `^UA\d{27}$`.
- `contacts[i].name` non-empty; `contacts[i].email` format-validated;
  at most one contact with `is_primary = true` (key `contacts`,
  code `multiple_primary_contacts`).
- `payment_terms_days` in `0..=365`; `default_discount_bp` in `0..=10000`.
- `credit_limit.amount_minor >= 0`; `default_currency` and
  `credit_limit.currency` must be 3 uppercase letters (same check orders
  use for currency).
- `tags`: service normalizes (trim, lowercase, drop empties, dedupe) —
  normalization is not a validation error.

Checksum validation of ЄДРПОУ/РНОКПП control digits is **out of scope**
(format-only for the prototype); note it in the code as a `NOTE:`.

### Status lifecycle (service-enforced, invalid transition → 409)

```
0 lead ──► 1 active ◄──► 2 inactive
              │                │
              ▼                ▼
           3 blocked ──────► 1 active   (unblock)
```

Allowed transitions: `0→1`, `1→2`, `2→1`, `1→3`, `2→3`, `3→1`. Everything
else → `409 conflict`.

Interaction with orders (enforced in the **order** service at creation):

- New order for a `blocked` (3) or `inactive` (2) customer → `409` with
  message "customer is not active".
- New order for a `lead` (0) auto-promotes the customer to `active` (1) in
  the same operation (a first order *is* the conversion). The promotion is
  a normal update, so it flows through live updates like any other change.
- Deletion rule unchanged: a customer with orders cannot be deleted (409),
  regardless of status. `blocked` is the "soft off" state.

## API contract (delta)

| Method + path | Purpose |
|---|---|
| `POST /api/customers/{id}/status` | body `{ "status": 1 }` — lifecycle transition |
| `GET /api/dictionaries/customer-statuses` | status metadata: `{ id, key, sort, color, can_order, allowed_targets, labels: {en, ua} }` |

`can_order` is `true` for statuses `0 | 1` (lead auto-promotes, see above).

**List parameters:** `GET /api/customers` additionally accepts `status`
(int) and `tag` (string, exact match against the normalized tag) filters —
same composition rules as the orders list's `customer_id`/`status` filters
(combine with `q`, pagination, sorting).

Create/update bodies are the extended entity minus `id` and timestamps.
`status` is **not** settable via PUT — transitions only via the
status route (mirrors orders). New customers start as `status = 1` (active)
by default; the create body may pass `status: 0` to register a lead —
those are the only two accepted creation statuses (anything else → 422).

## Full-text search (delta)

Replace `customer_search` (migration, `REMOVE INDEX` + `DEFINE INDEX`) to
cover the new identity fields:

```sql
DEFINE INDEX customer_search ON customer
  FIELDS name, legal_name, edrpou, contacts[*].name, contacts[*].email
  SEARCH ANALYZER autocomplete BM25 HIGHLIGHTS;
```

The per-entity `q` filter and the omnibox both match the same fields.
Searching a ЄДРПОУ fragment or a contact's name must find the customer.
Note the `autocomplete` analyzer's `ascii` filter folds Cyrillic — verify
during Step 2 that Ukrainian names tokenize and match (they did for the M4
seed data via the existing `name` field; the new fields use the same
analyzer, this is a regression check, not new ground).

## Migration (tenant db) — `0009_customers_crm.surql`

One ordered migration, applied per tenant at provisioning/startup like all
others. It must be **idempotent** (guard on the fields it introduces) and
handle legacy rows:

1. Backfill scalars: `status = 1`, `kind = 0`, `tags = []`,
   `payment_terms_days = 0`, `default_discount_bp = 0`. Do **not** backfill
   `default_currency` — the tenant's default currency isn't readable from
   inside a per-tenant `.surql` file, and hardcoding one would be wrong for
   other tenants. Instead the store's Row→domain conversion treats a
   missing `default_currency` as "use the tenant default" (filled from the
   request's tenant settings); record this read-repair in a comment on the
   conversion.
2. Contacts: rows with any of `contact_name/email/phone` set get
   `contacts = [{ name: contact_name ?? email ?? phone, role: NONE, email,
   phone, is_primary: true }]`; rows without get `contacts = []`. Then
   `UNSET contact_name, email, phone`.
3. Addresses: `legal_address = address`, `delivery_address = NONE`, then
   `UNSET address`. (The legacy single address is treated as the legal
   address; delivery stays empty until staff fill it.)
4. FTS: `REMOVE INDEX IF EXISTS customer_search ON customer;` then the new
   `DEFINE INDEX` above.

No customer numbering and no `customer_prefix` — see
docs/adr/0011-drop-customer-numbering.md.

## Frontend (delta)

- `features/customers/types.ts`: extend the zod schema (source of truth);
  kind/status as z.number() enums; `contacts` as an array schema; money via
  the existing money schema. Drop the removed legacy fields.
- `Form.tsx`: sectioned form (Mantine `Fieldset` or tabs):
  1. **Загальні дані** — kind (SegmentedControl: Юр. особа / ФОП / Фіз.
     особа), name, legal_name, edrpou / tax_id (shown conditionally by
     kind), vat_ipn, industry, source, website, tags (Mantine `TagsInput`).
  2. **Контакти** — inline-editable rows (add/remove, primary radio),
     same interaction pattern as the order form's line items.
  3. **Адреси** — legal + delivery, each the existing address sub-form,
     country defaulting to `UA`.
  4. **Фінанси** — payment_terms_days, credit_limit (money input via
     `lib/money` decimal-string conversion), default_currency select,
     default_discount_bp (rendered as % with bp conversion at the
     boundary), iban, bank_name.
  Field-level API validation errors map onto nested paths
  (`contacts.0.email`, `legal_address.country`) — same mechanism the order
  form uses for line items.
- `Detail.tsx`: status badge (color from the dictionary) + transition
  buttons driven by `allowed_targets` from
  `/api/dictionaries/customer-statuses` — copy the order Detail's
  transition UI, including the 409 toast and the optimistic
  update/rollback wiring from M5 Step 6.
- `List.tsx`: columns name, ЄДРПОУ/РНОКПП (one column, whichever
  is set), status badge, tags, primary contact; status filter
  (Select fed by the dictionary) and tag filter next to the search box —
  same layout as the orders list's customer/status filters.
- Order form (M4.1 customer selector): customers with `can_order = false`
  statuses are filtered out of the selector; if the API still returns 409
  (race), surface the toast.
- i18n: all new labels in `customers` namespace, `en` + `ua`, `ua`
  default. Status labels come from the dictionary endpoint, not local
  translation files (same as order statuses).
- Live updates: no changes needed — the WS hub streams the whole customer
  entity; the extended struct serializes through `ChangeEvent<Customer>`
  automatically. Verify in the acceptance pass, don't build anything.

## Seeder (delta)

Extend the Ukrainian demo tenant generator (M4: 100 customers / 1000
orders): realistic mix of kinds (~60% ТОВ, ~35% ФОП, ~5% фіз. особа),
valid-format ЄДРПОУ/РНОКПП/ІПН, 1–3 contacts each (one primary), UA
addresses, tags from a small pool («поліграфія», «постійний», «опт», …),
payment terms 0/7/14/30, a few blocked/inactive/lead statuses so the
filters demo well. The 50k perf tenant gets the same generator (perf
numbers must be re-checked against the new FTS index — see Step 6).

---

## Build order

Each step leaves `just check` green; integration tests land with the step
that makes them testable. Backend integration tests run per PLAN.md's
harness (shared container, fresh tenant per test, `#[ignore]`).

### Step 1 — Domain model + validation + lifecycle

`crates/domain/src/customer.rs`:

- Extend `Customer`/`NewCustomer` with all new fields; add
  `CustomerKind` and `CustomerStatus` enums with `code()`/`key()`
  mirroring `OrderStatus` (integer wire format, string keys for the
  dictionary and i18n).
- `NewCustomer::validate_domain` grows the kind-conditional rules above;
  tag normalization as a service-side `normalize()` step.
- `pub fn can_transition(from: CustomerStatus, to: CustomerStatus) -> bool`
  and `pub fn can_order(status: CustomerStatus) -> bool` — pure functions,
  exhaustively unit-tested (every pair asserted, like the order transition
  tests).
- Unit tests: each validation rule (valid + invalid case), kind
  conditionality (edrpou on a ФОП rejected), multiple-primary-contacts,
  IBAN format, transition matrix.

### Step 2 — Migration + store

- `0009_customers_crm.surql` exactly as specced above.
- `customer_repo.rs`: extend the Row struct + Row→domain conversion
  (including the `default_currency` read-repair from tenant settings);
  `list` gains `status`/`tag` filters (bound parameters, composed like the
  order repo's filters).
- Integration tests (`#[ignore]`): (a) legacy-shaped record written
  pre-migration migrates to the new shape — contacts array, legal_address,
  backfilled status, legacy keys gone; (b) migration is idempotent
  (run twice, same result); (c) FTS finds a customer by ЄДРПОУ fragment,
  contact name, and Ukrainian legal_name prefix; (d) list filters by
  status and tag.

### Step 3 — (removed) Numbering

This spec originally assigned each customer a `CUS-000123`-style number via
a `customer_prefix` tenant setting, mirroring order/invoice numbering. That
was dropped before it proved out — see
docs/adr/0011-drop-customer-numbering.md for why — so this step no longer
exists; there is no `customer.number`, `customer_prefix`, or numbering
service call anywhere in the entity.

### Step 4 — API routes + order-service guard

- `POST /api/customers/{id}/status` — handler → service transition (404 /
  409 / 200 with the updated entity), copied structurally from the order
  status route.
- `GET /api/dictionaries/customer-statuses` in `routes/dictionaries.rs`,
  same shape as `order_statuses` with `can_order` instead of
  `invoiceable`; labels en + ua.
- Customer create accepts `status` 0|1 only (422 otherwise); PUT ignores
  `status`.
- Order service: creation checks the customer's status — 409 for 2|3,
  lead auto-promote for 0 (promotion happens before the order insert, and
  its live event must reach clients — assert in the integration test).
- Integration tests: full CRUD round-trip of an extended customer through
  the API (all fields survive); status transition happy path + 409 on
  invalid; order creation blocked for a blocked customer; order creation
  for a lead promotes it (customer status 1 afterwards + both WS events
  observable via the existing WS test harness).

### Step 5 — Frontend

Everything under "Frontend (delta)" above: types, form sections, contacts
editor, status UI on Detail, list columns/filters, order-form selector
filtering, i18n en+ua. Vitest: zod schema round-trip of a full customer;
contacts editor add/remove/primary logic; discount bp ↔ % and credit-limit
minor-units conversions; one optimistic status-transition rollback test
(mirroring the M5 Step 6 pattern).

### Step 6 — Seeder + acceptance + perf re-check

- Seeder deltas for both demo tenants.
- Acceptance pass: create a ФОП lead with two contacts in the UI (ua
  locale, no raw keys); order for it auto-promotes to active and both
  changes appear live in a second browser; blocked customer is absent
  from the order form's selector; omnibox finds a customer by ЄДРПОУ.
- Re-run the `perf-check` skill on the seeded volume: `/api/customers?q=`
  and `/api/search` p95 must stay < 100 ms with the wider FTS index;
  record the numbers in `docs/perf.md`. If the index over the contact
  arrays blows the budget, drop `contacts[*].email` from the index first
  (least demo value) and record the decision as an ADR.
- Record any deviations from this spec as an ADR under `docs/adr/`.
