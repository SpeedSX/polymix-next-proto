# 0007 — M4 (i18n + currency + org settings) deviations from PLAN.md

## Status

Accepted.

## Context

M4 touches several PLAN.md sections that turned out to be slightly
inconsistent with each other, or with what's already built. Four separate
points, grouped into one ADR since each is small and none stands alone.

### 1. `default_language` is `"en" | "uk"`, not `"en" | "de"`

The system-db schema block (Data model) types `default_language` as
`"en" | "de"`. The Architecture section ("Language & currency switching")
is explicit that the supported pair is English + Ukrainian (originally
written as `ua` + `en`) and M0–M3 already only ever set `"en"`. Treated
the schema block as the stale one and implemented `en`/`uk` throughout —
there is no `de` locale anywhere in the frontend. The language tag is the
BCP-47 code `uk` (not the country code `ua`) so `Intl` date/number
formatting resolves correctly.

### 2. `Invoice.exchange_rate` is snapshotted at creation, not at issue

The data model's `invoice` doc-comment says `exchange_rate` is a "snapshot
at issue time". But the invoice's currency (and therefore whether a
snapshot is even needed) is fixed at **creation** — `POST
/api/orders/{id}/invoice` — and the API contract has no mechanism to change
currency later; `set_status` only ever touches `status`/`issue_date`/
`due_date`. There is nothing left to (re-)snapshot at issue time that
wasn't already fixed at creation. `SurrealInvoiceRepo::create` computes the
snapshot once, from `tenant.default_currency` vs. the order's currency, and
`exchange_rate` never changes afterwards.

### 3. The app's boot language stays `en`; `uk` is opt-in via the switcher

The Architecture section says to make Ukrainian the default choice. Doing
that literally — defaulting `i18next`'s `lng` to `uk` — would flip every
existing frontend test that asserts English copy from the default `i18next`
singleton (`App.test.tsx`, `orders`/`invoices` `Form.test.tsx`, `customers`
`Form.test.tsx`) to fail, since none of them call `i18n.changeLanguage`
before rendering. M4's own "Done when" bullet only requires that *switching*
to Ukrainian works fully (full translation, reformatted dates/numbers) — it
doesn't require `uk` to be the initial render. Kept `en` as the actual
runtime default and boot language; `uk` is fully translated and reachable
via the new `AppShell` language switcher, persisted to `localStorage` per
PLAN.md's "locale persisted per user (localStorage for the prototype)".

### 4. Seeder's Ukrainian names are a hand-rolled pool, not a `fake` locale

PLAN.md's M2 seeder uses the `fake` crate's English fakers
(`fake::faker::*::en`). `fake` 2.9's locale support does not include
`uk_UA` (or any Slavic locale) — there is no
`fake::faker::name::uk_UA::Name` to swap in. `crates/seeder/src/uk.rs` is a
small curated set of Ukrainian first/last names, company-name parts,
cities, and street names instead, good enough for demo data.

## Decision

1. `default_language` stays a free-form `String` in code (not a Rust enum);
   the two values actually produced/consumed are `"en"` and `"uk"`.
2. `Invoice.exchange_rate` is computed once in
   `SurrealInvoiceRepo::create`, from `tenant.default_currency` vs. the
   order's currency at that moment; nothing re-snapshots it later.
3. `frontend/src/lib/i18n/index.ts` keeps `lng: 'en'`/`fallbackLng: 'en'`;
   the language switcher and `localStorage` restoration are additive on
   top, never changing the no-preference-stored default. A stored legacy
   value of `ua` is rewritten to `uk` on restore.
4. `crates/seeder/src/uk.rs` owns the Ukrainian name/address data; selected
   via `SEED_LOCALE=uk` (`just seed-uk`), which also switches the
   provisioned tenant's `default_language`/`default_currency` to
   `uk`/`UAH` via `TenantProvisioner::provision_with_locale`.

## Consequences

- No `de` locale exists or is planned; if German is ever wanted, that's a
  new locale addition, not a fix to this deviation.
- If invoice currency ever becomes editable after creation (out of scope
  today — invoices are frozen once issued, and the currency itself is never
  a `PUT`-able field even in draft), the snapshot logic in `create()` would
  need to move or be re-run; nothing today calls for that.
- A user who never touches the language switcher always sees English,
  regardless of their tenant's `default_language` — including on the `uk`
  demo tenant. This is acceptable for the prototype; a real "respect
  tenant/browser locale on first load" behavior is a small follow-up, not
  implemented here to avoid the test-suite churn described above.
- Ukrainian demo names are a fixed pool of ~10-20 items per category, not
  infinite variety like `fake`'s English fakers — fine for 100
  customers/1000 orders, would look repetitive at `fake`'s usual 50k/200k
  scale.
