# PolyMix Next — Prototype Implementation Plan

Next-generation multi-tenant management information system (MIS) for print shops and typesetting companies. This prototype proves the architecture and the main NFRs; it is not the production system.

## How to work this plan

Rules for the implementing agent:

- Work **one milestone at a time**, in order. A milestone is done only when every item in its "Done when" list passes; do not start the next one before that.
- After every task, the project must still build and all existing tests must pass (`just check`, see [Commands](#commands)).
- When this plan and reality conflict (an API changed, a crate is broken), prefer the plan's *intent*, record the deviation in an ADR under `/docs/adr/`, and keep going. Do not silently redesign.
- Do not add features, entities, or endpoints beyond this document. "Out of scope" means out of scope.
- Use the exact names (routes, fields, env vars, file paths) given below — other parts of the system are specified against them.

## Decisions

| Concern | Choice | Rationale |
|---|---|---|
| Backend | Rust — Axum + Tokio | Performance NFR, fun-in-development, mature async ecosystem |
| Database | SurrealDB | `LIVE SELECT` covers event-driven UI updates natively; built-in full-text search indexes; database-per-tenant isolation |
| Frontend | React 19 + TypeScript + Mantine 8 + TanStack (Query, Table, Router) + Vite | Clean functional design, richest control set, data-grid strength for MIS screens |
| Auth | Clerk (hosted), provider-agnostic JWT middleware | Drop-in React components; Clerk Organizations map 1:1 onto tenants (org id arrives as a JWT claim); backend only verifies JWTs against JWKS, so the provider is swappable via config |
| Live updates | WebSocket from backend (fan-out of SurrealDB live queries) | Keeps authorization in the backend; clients never talk to the DB directly |
| i18n / currency | i18next + ICU message format; amounts stored as minor units + ISO 4217 code | Language and currency switching NFR |
| Deployment | Docker images, Docker Compose locally, Fly.io for the cloud prototype | Cheapest portable path; images move to any cloud later |
| Scope | Customers, Orders, Invoices — CRUD, full-text search, live updates | Minimum surface that proves every NFR |

## Architecture

```
┌────────────────────────────┐
│ React SPA (Mantine)        │
│  TanStack Query + Router   │
│  i18next, WS client        │
└─────────┬───────────▲──────┘
          │ REST/JSON │ WebSocket (entity change events)
┌─────────▼───────────┴──────┐
│ Rust API (Axum)            │
│  auth (JWT, tenant claim)  │
│  handlers → services →     │
│  repositories (trait)      │
│  live-query fan-out hub    │
└─────────┬──────────────────┘
          │ surrealdb crate (WS protocol)
┌─────────▼──────────────────┐
│ SurrealDB                  │
│  ns: polymix               │
│  db per tenant             │
│  FTS indexes, LIVE SELECT  │
└────────────────────────────┘
```

### Multi-tenancy

- One SurrealDB namespace (`polymix`), **one database per tenant**. Hard isolation, no `WHERE tenant_id = …` on every query, and live queries are automatically tenant-scoped.
- Tenant database name is deterministic: `tenant_` + first 12 hex chars of SHA-256 of the org id (e.g. `tenant_3f2a9c81d04b`). Never derive it from the org *name* (names change, ids don't).
- Clerk handles sign-in, users, and tenant membership: one Clerk **Organization** per tenant, so the org switcher, invitations, and member management come from Clerk, not from us. The active org id is a claim in the session JWT.
- An Axum middleware verifies the JWT against the issuer's JWKS and resolves the org claim to the tenant database. Its contract is "validated JWT in, `AuthContext { user_id, org_id, tenant_db }` out"; issuer URL and JWKS endpoint are config, so swapping Clerk for a self-hosted provider (Zitadel/Logto) later is a config change plus a frontend component swap.
- **Auto-provisioning:** on the first authenticated request with an unknown org id, the backend creates the tenant database, runs migrations on it, and inserts a registry record — guarded by a per-org-id async mutex so concurrent first requests don't provision twice.
- Tenant registry (org id → tenant db mapping, settings: default language, default currency) lives in a shared `system` database in the same namespace.

### Live updates (event-based UI)

1. On the first WebSocket subscriber per tenant, the backend opens `LIVE SELECT` on `customer`, `order`, `invoice` in that tenant's database (and closes them when the last subscriber disconnects).
2. A hub task fans incoming change notifications out to WebSocket sessions of that tenant, as `{entity, action: create|update|delete, id, data}` envelopes.
3. Frontend WS client invalidates/patches the matching TanStack Query caches — lists and detail views update without polling.
4. Optimistic UI on mutations; the live event is the reconciliation signal.

### Full-text search

- `DEFINE ANALYZER` (lowercase, ascii folding, edge-ngram for search-as-you-type) + `DEFINE INDEX … SEARCH ANALYZER … BM25 HIGHLIGHTS` on the searchable fields of each main table.
- Per-entity search via the `q` parameter on list endpoints, plus a global omnibox endpoint that queries all three tables and merges ranked results.
- Known limitation vs Meilisearch: no typo tolerance. Edge-ngram prefix matching mitigates this for the prototype; the repository trait keeps a swap possible (see Risks).

### Language & currency switching

- **Language:** i18next with ICU messages, `ua` + `en` (typical print-market pair) to prove the mechanism; locale persisted per user (localStorage for the prototype); all dates/numbers via `Intl` with the active locale. Make 'ua' default choice.
- **Currency:** every money field is `{amount_minor: int, currency: string}`. Tenant has a default currency; invoices carry their own currency plus an exchange rate snapshot at issue time (rates table in the tenant DB, seeded statically for the prototype). Display formatting via `Intl.NumberFormat`.

## Repository layout

```
/backend                    Rust workspace (single Cargo.toml workspace root)
  /crates/api               Axum app: routes, extractors, middleware, WS hub, dev token issuer
  /crates/domain            entities, services, repository traits, domain errors
  /crates/surreal-store     repository implementations, migrations (.surql), FTS defs
  /crates/seeder            binary: fake-data generator (M2)
/frontend                   Vite + React + TS
  /src/features/{customers,orders,invoices,search}
  /src/lib/{api,ws,auth,i18n,money}
/deploy                     Dockerfiles, fly.toml, compose.yaml
/docs                       ADRs (one per Decisions row) in /docs/adr, perf notes
justfile                    all dev commands (see Commands)
```

Backend crate seams matter: `domain` depends on nothing SurrealDB-specific, so the store is swappable and testable.

---

# Specifications

Everything below is the contract the implementation must follow. When a milestone says "customers CRUD", these sections define exactly what that means.

## Data model

Conventions for all entities:

- **Ids:** ULID strings, generated by the store. SurrealDB record id is `customer:<ulid>`; the API exposes only the key part (`"01HZY3…"`) as `id` — clients treat it as opaque, the store re-prefixes the table name.
- **Timestamps:** `created_at`, `updated_at` on every entity, RFC 3339 UTC strings, set by the store (`updated_at` on every write).
- **Money:** `{ "amount_minor": 12345, "currency": "EUR" }` — integer minor units (cents), ISO 4217 code. Never floats.
- **Document numbers** (`ORD-000123`, `INV-000042`): per-tenant sequences via a counter record — `UPSERT counter:order SET value += 1 RETURN value;` — formatted with a 6-digit zero-padded value. Assigned by the service layer at creation, immutable afterwards.

### system database

```
tenant {
  id: ulid,
  org_id: string,          // provider org id, unique
  db_name: string,         // "tenant_<12 hex>"
  name: string,            // display name, from the org at provision time
  default_language: "en" | "de",     // default "en"
  default_currency: string,          // ISO 4217, default "EUR"
  created_at, updated_at
}
```

### tenant database

```
customer {
  id: ulid,
  name: string (required, non-empty),        // company name
  contact_name: string | null,
  email: string | null (format-validated),
  phone: string | null,
  address: { street, zip, city, country } | null   // country: ISO 3166-1 alpha-2
  notes: string | null,
  created_at, updated_at
}
// M5.1 extends customer into a Ukraine-focused CRM profile (legal ids,
// lifecycle status, embedded contacts, commercial terms) — once M5.1 lands,
// docs/customers-crm.md is normative for this entity and supersedes the
// block above.

order {
  id: ulid,
  number: string,                 // "ORD-000123", unique per tenant
  customer_id: ulid (required, must exist),
  status: 0 | 1 | 2 | 3 | 4,       // order status id (see /api/dictionaries/order-statuses)
  currency: string,               // defaults to tenant default currency
  line_items: [                   // 1..n
    { description: string, quantity: int > 0, unit_price: money }
  ],
  total: money,                   // computed by the service: Σ quantity × unit_price; never trusted from the client
  notes: string | null,
  created_at, updated_at
}

invoice {
  id: ulid,
  number: string,                 // "INV-000042", unique per tenant
  order_id: ulid (required),
  customer_id: ulid,              // denormalized from the order
  status: "draft" | "issued" | "paid" | "void",
  currency: string,               // may differ from the order's currency (M4)
  exchange_rate: string | null,   // decimal string, snapshot at issue time; null when currency == tenant default. Informational only — display conversions happen on the frontend.
  line_items: [ { description, quantity, unit_price: money } ],   // copied from the order at creation, then independent
  net_total: money,
  tax_rate_bp: int,               // basis points, e.g. 1900 = 19% VAT; default 1900
  tax_total: money,               // round(net_total × tax_rate_bp / 10000), half-up, on the total (not per line)
  gross_total: money,
  issue_date: date | null,        // set when status → issued
  due_date: date | null,          // issue_date + 14 days by default
  created_at, updated_at
}

exchange_rate {                   // seeded statically at tenant provisioning
  id: ulid,
  base: string, quote: string,    // e.g. base EUR, quote USD
  rate: string                    // decimal string, e.g. "1.0842"
}

counter { id: "order" | "invoice", value: int }
```

Status transitions (service-enforced; invalid transition → `409 conflict`):

- order: `0 → 1 → 2 → 3`; `4` reachable from `0|1`; no other moves.
- invoice: `draft → issued → paid`; `void` reachable from `draft|issued`. An invoice can be created only from an order with status `1|2|3`; one invoice per order (second attempt → `409`).
- Deleting: customers with orders and orders with invoices cannot be deleted (`409`). Invoices are never deleted — void them.

### SurrealDB definitions (in `/crates/surreal-store/migrations/*.surql`)

Migrations are ordered files `0001_init.surql`, `0002_….surql`, applied per tenant database at provisioning and at startup; the applied version is stored in a `meta:migrations` record. The analyzer + FTS indexes (M3):

```sql
DEFINE ANALYZER autocomplete TOKENIZERS class FILTERS lowercase, ascii, edgengram(2, 10);

DEFINE INDEX customer_search ON customer
  FIELDS name, contact_name, email, address.city
  SEARCH ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX order_search   ON order   FIELDS number, notes, line_items[*].description
  SEARCH ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX invoice_search ON invoice FIELDS number
  SEARCH ANALYZER autocomplete BM25 HIGHLIGHTS;
```

Search query shape (per entity, `$q` bound, never string-interpolated):

```sql
SELECT *, search::score(0) AS score FROM customer
WHERE name @0@ $q OR contact_name @0@ $q OR email @0@ $q
ORDER BY score DESC LIMIT $limit;
```

## API contract

Base path `/api`, JSON everywhere. All routes except `/api/health` require `Authorization: Bearer <jwt>`.

| Method + path | Purpose |
|---|---|
| `GET /api/health` | liveness: `{"status":"ok"}`, no auth, no dependencies |
| `GET /api/ready` | readiness (M6): DB ping, no auth — see Operational hardening |
| `GET /api/me` | `{ user_id, org_id, tenant: { name, default_language, default_currency } }` |
| `GET /api/customers` | paged list, params below |
| `POST /api/customers` | create; body = entity minus id/timestamps |
| `GET /api/customers/{id}` | detail |
| `PUT /api/customers/{id}` | full update (no PATCH in the prototype) |
| `DELETE /api/customers/{id}` | delete (409 if referenced) |
| same five routes | `/api/orders`, `/api/invoices` |
| `POST /api/orders/{id}/status` | body `{ "status": 1 }` — transition |
| `POST /api/orders/{id}/invoice` | create invoice from order; body `{ currency?: string }` |
| `POST /api/invoices/{id}/status` | body `{ "status": "issued" }` |
| `GET /api/search?q=` | global omnibox (M3), shape below |
| `GET /api/ws?token=<jwt>` | WebSocket upgrade (M5) |
| `GET /api/dictionaries/order-statuses` | order status metadata (ids, labels, transitions, invoiceability) |

**List parameters:** `page` (1-based, default 1), `limit` (default 25, max 100), `sort` (field name, prefix `-` for desc, default `-created_at`), `q` (FTS filter, M3+; when present results are ranked by score and `sort` is ignored). Orders/invoices additionally accept `customer_id` and `status` filters.

**List response:** `{ "items": [...], "total": 1234, "page": 1, "limit": 25 }` (`total` from a parallel `count()` query).

**Global search response:** `{ "customers": [{id, label, highlight}], "orders": [...], "invoices": [...] }` — max 5 per entity, `label` is name/number, `highlight` is the BM25-highlighted fragment (safe subset: only `<b>` tags).

**Errors** — one envelope, correct status codes:

```json
{ "error": { "code": "validation_failed", "message": "…", "details": { "email": "must be a valid email" } } }
```

Codes: `unauthorized` (401), `forbidden` (403), `not_found` (404), `validation_failed` (422), `conflict` (409), `internal` (500 — generic message, real error only in server logs). Validation runs in the domain layer (not just the frontend).

## Auth

**JWT claims** the middleware requires: `iss` (must equal `AUTH_ISSUER`), `sub` (user id), `exp`, and the org claim — claim name is config (`AUTH_ORG_CLAIM`, default `org_id`). Signature verified against JWKS fetched from `AUTH_JWKS_URL`, cached in memory, refreshed on unknown `kid` (at most once per 5 minutes). RS256 only. Requests with a valid JWT but **no org claim** get `403` with code `forbidden` and message "no active organization".

**Dev issuer** (M0): compiled into the api crate, enabled only when `AUTH_DEV_MODE=true`. Serves `GET /dev/jwks.json` (its own generated RSA key) and `POST /dev/token` with body `{ "user_id": "user_dev1", "org_id": "org_dev1" }` → `{ "token": "…" }` (24 h expiry, same claim shape as Clerk). In dev mode `AUTH_ISSUER`/`AUTH_JWKS_URL` point at the app itself. The frontend, when `VITE_AUTH_MODE=dev`, shows a plain "dev sign-in" form (user id + org id inputs) instead of Clerk components and stores the token in memory. CI and all backend integration tests use the dev issuer; nothing in CI talks to Clerk.

**Clerk** (M1): default Clerk session tokens include `org_id` when an active organization is set — no custom template needed. Frontend uses `@clerk/clerk-react`: `<SignIn/>`, `<OrganizationSwitcher/>`, and `getToken()` per request (Clerk rotates short-lived tokens; never cache one).

## WebSocket protocol (M5)

- Connect: `GET /api/ws?token=<jwt>` (browsers can't set headers on WS). Invalid/missing token → close before upgrade with HTTP 401.
- Server → client: `{ "type": "change", "entity": "customer|order|invoice", "action": "create|update|delete", "id": "<ulid>", "data": <entity or null for delete> }` and `{ "type": "ping" }` every 30 s.
- Client → server: `{ "type": "pong" }`. No other client messages; subscriptions are implicit (you get your tenant's three tables, nothing else).
- Client reconnect: exponential backoff 1 s → 30 s cap; **on every reconnect the client invalidates all entity queries** (`queryClient.invalidateQueries()`) so missed events never leave stale UI.
- Cache mapping: `change` on entity X invalidates list queries `['customers']` etc. and patches/invalidates detail query `['customers', id]`.
- The hub (backend): one task per tenant owning the three live queries; drops the live queries when the tenant's subscriber count hits zero; on SurrealDB connection loss, re-establishes live queries and broadcasts `{ "type": "resync" }` so clients refetch.

## M5 work breakdown (Live updates)

The WebSocket protocol section above is the contract; this section is the build order. Each step leaves `just check` green; integration tests land with the step that makes them testable, not at the end.

### Step 1 — Backend: token validation reusable outside the header middleware

`require_auth` in `crates/api/src/auth.rs` currently owns the whole chain (bearer header → JWT validation → org claim → tenant resolution/provisioning → `AuthContext`). The WS route gets its token from the `?token=` query parameter, so:

- Extract the header-independent core into `authenticate_token(state, token) -> Result<AuthContext, AuthError>` (JWT validation against the JWKS cache, org-claim check, tenant resolution incl. auto-provisioning). `require_auth` becomes "read header → call it"; the WS handler calls it directly with the query param.
- No behavior change; the existing auth unit tests must pass unmodified. Add one test asserting header extraction and token validation fail independently (bad header vs bad token).

### Step 2 — Backend: typed live-change streams in `surreal-store`

The api crate must not see `RecordId`/row structs (they're private to the repos, and the layering rule says no SurrealDB types outside the store). Add `crates/surreal-store/src/live.rs`:

- `pub enum LiveChange { Customer(ChangeEvent<Customer>), Order(ChangeEvent<Order>), Invoice(ChangeEvent<Invoice>) }` with `pub struct ChangeEvent<T> { pub action: ChangeAction /* Create|Update|Delete */, pub id: String, pub data: Option<T> }` — `data` is `Some(entity)` for create/update, `None` for delete (matches the protocol's `data: null`).
- `pub async fn live_changes(session: Arc<Surreal<Any>>) -> Result<impl Stream<Item = LiveChange>, DomainError>` — opens `LIVE SELECT` on `customer`, `order`, `invoice` via the SDK's `.select(table).live()` streams, merges them (`futures::stream::select_all`), and maps notifications through the same Row→domain conversions the repos use (move those `From<…Row>` impls to a shared module rather than duplicating).
- Dropping the returned stream must kill the live queries server-side (the SDK does this on stream drop — verify with a test, it's the mechanism the hub's teardown relies on).
- Integration test (`#[ignore]`, shared container): open the stream on a fresh tenant db, create/update/delete a customer through the repo, assert three `LiveChange`s with correct action, id (key part, not `customer:…`), and mapped data.

### Step 3 — Backend: the hub

New `crates/api/src/ws/hub.rs`, owned by `AppState` as `Arc<Hub>`:

- `Hub { tenants: Mutex<HashMap<String, TenantEntry>>, store: Arc<Store> }`; `TenantEntry { tx: broadcast::Sender<Arc<ServerEvent>>, subscribers: usize, task: JoinHandle<()> }`. `ServerEvent` is the serialized protocol envelope (`change` | `resync`); broadcast capacity 256.
- `subscribe(tenant_db) -> broadcast::Receiver<…>`: under the lock, bump the count; on 0→1 spawn the tenant task. `unsubscribe(tenant_db)`: decrement; on 1→0 abort the task and remove the entry — same lock guards both, so a subscribe racing an unsubscribe either finds the live entry or creates a fresh one, never a half-dead one.
- Tenant task: `store.for_tenant(db)` for its own long-lived session (per the tenant-session rules — never shared with request traffic), then loop: open `live_changes`, forward each mapped envelope into `tx`. If the stream ends or errors (SurrealDB restart), retry with backoff 500 ms → 5 s (cap), and after a successful re-open broadcast `{ "type": "resync" }` so clients refetch what they missed. Log reconnect attempts at `warn`.
- Unit-testable seam: the task body takes the stream-factory as a closure so hub lifecycle (spawn on first, abort on last, resync after factory error) is testable without a database.

### Step 4 — Backend: WS route

`crates/api/src/ws/handler.rs`, wired as `GET /api/ws` **outside** the `require_auth` middleware layer (it authenticates itself):

- Extract `token` from the query string; missing/invalid → the appropriate error envelope status (401/403) *before* `ws.on_upgrade` — the protocol requires rejection pre-upgrade.
- Per connection: `hub.subscribe(auth.tenant_db)`, then a select loop over (a) broadcast receiver → forward JSON text frames, (b) 30 s interval → send `{ "type": "ping" }`, (c) incoming messages → accept `{ "type": "pong" }` and close frames, ignore anything else. On `RecvError::Lagged` send one `resync` instead of dropping the connection. Any send error → break.
- `hub.unsubscribe` on exit via a guard type, so panics and normal exits both release the slot.
- Integration tests (api crate, `#[ignore]`, real router + `tokio-tungstenite` client against a bound listener — `oneshot` can't carry a WS upgrade):
  - no/invalid token → HTTP 401 on the upgrade request;
  - create/update/delete a customer via REST → connected client receives the three envelopes in order, delete has `data: null`;
  - **tenant isolation (mandatory):** clients on org A and org B; a mutation in A reaches A's client and, within a 2 s window, nothing arrives at B's;
  - resilience: pause/unpause the SurrealDB container → client receives `resync` and a subsequent mutation's event still arrives.

### Step 5 — Frontend: WS client + cache mapping

New `frontend/src/lib/ws/`:

- `WsClient`: connects to `VITE_WS_URL + '/api/ws?token=' + await getToken()` — the token is fetched fresh on **every** (re)connect attempt (Clerk rotates short-lived tokens; a cached one would fail after ~60 s). Reconnect with exponential backoff 1 s → 30 s cap, reset on successful open. Replies `{"type":"pong"}` to pings.
- Cache mapping in one place (`lib/ws/applyChange.ts`), driven by the existing query-key modules: `change` on entity X → `invalidateQueries(xKeys.all)` for lists; detail: update with `data` → `setQueryData(xKeys.detail(id), data)`, create → nothing extra (lists cover it), delete → `removeQueries(xKeys.detail(id))`. `resync` **and every reconnect-open after a drop** → `queryClient.invalidateQueries()` (missed events must never leave stale UI).
- Mounted as `<LiveUpdatesProvider>` inside the auth provider (needs `getToken`) and the QueryClient provider; disconnects on sign-out/org switch and reconnects with the new token (org switch changes the tenant — a stale socket would stream the wrong tenant's events).
- Vitest: `applyChange` against a real `QueryClient` (all four action×entity mappings, plus resync); reconnect backoff with fake timers and a mock WebSocket.

### Step 6 — Frontend: optimistic updates

Scope: edit-shaped mutations only — customer/order/invoice edit forms and the status-transition buttons. Creates stay invalidate-on-success (no id to patch yet).

- Standard TanStack pattern per mutation: `onMutate` cancels in-flight queries for the detail key, snapshots, `setQueryData` with the optimistic value; `onError` restores the snapshot; `onSettled` invalidates the detail + list keys. The WS `change` event is the reconciliation signal — no special handling needed, the mapping from Step 5 already applies the server truth.
- Status transitions must render the optimistic status instantly and roll back visibly on 409 (the invalid-transition toast from M2 remains the error surface).
- Vitest: one mutation's optimistic→rollback path with a mocked failing fetch.

### Step 7 — Acceptance pass

- Two browsers, same dev tenant: edit in one appears in the other < 1 s on list and detail without user action.
- Browser on a second tenant sees nothing (manual mirror of the automated isolation test).
- `<runtime> compose … restart surrealdb` mid-session: both browsers recover live updates without a page reload (resync path).
- Record any deviations from the WS protocol section as an ADR; update `docs/perf.md` only if the hub changes search/list latencies (it shouldn't).

## Configuration

Backend (env vars, read once at startup, fail fast if required ones are missing):

| Var | Default | Notes |
|---|---|---|
| `PORT` | `8080` | |
| `SURREALDB_URL` | `ws://localhost:8000` | |
| `SURREALDB_USER` / `SURREALDB_PASS` | `root`/`root` | root user, prototype only |
| `SURREALDB_NS` | `polymix` | |
| `AUTH_ISSUER` | — | required |
| `AUTH_JWKS_URL` | — | required |
| `AUTH_ORG_CLAIM` | `org_id` | |
| `AUTH_AUDIENCE` | — | optional; unset disables `aud` validation |
| `AUTH_DEV_MODE` | `false` | enables dev issuer |
| `CORS_ALLOWED_ORIGINS` | — | M6; comma-separated exact origins. Unset: permissive in dev mode, **startup error** otherwise |
| `RUST_LOG` | `info,api=debug` | tracing-subscriber env filter |

Frontend (Vite env): `VITE_API_URL`, `VITE_WS_URL`, `VITE_AUTH_MODE` (`clerk` | `dev`), `VITE_CLERK_PUBLISHABLE_KEY`.

## Operational hardening (M6)

Three changes that make the API survive cloud conditions (no compose healthcheck to hide behind, real browsers, real orchestrator probes). All three land in M6, before the Fly.io deploy.

### Startup retry in `Store::connect`

On Fly.io the API can start before SurrealDB accepts connections; `Store::connect` currently fails fast and the process dies.

- Wrap the **whole** startup sequence (connect → signin → `use_ns`/`use_db` → system-db DEFINEs) in a retry loop. A partial success (connected but signin failed) restarts the sequence from the top — the steps are all idempotent.
- Backoff: start at 500 ms, double per attempt, cap at 5 s; give up after a total deadline of 30 s and return the last error (the process exits; the orchestrator restarts it — that is the correct behavior past the deadline).
- Log **every** failed attempt at `warn` with the attempt number and the error text. Retry all error kinds: reliably telling transient errors from misconfiguration apart via SDK error types isn't worth the fragility, and the warn logs make a wrong password visible on attempt 1 even while retrying. This is a deliberate trade-off — record it in a comment.
- Hardcode the three durations as named constants; they are not config (keep the env surface small).
- Scope is startup only. Runtime connection loss stays the WS hub's job (see the WebSocket spec: re-establish live queries + broadcast `resync`); repos surface runtime errors as 500s.
- `just dev` keeps `compose up -d --wait` — locally the retry loop is a fallback, not the primary mechanism.
- Test: integration test that starts the API container before SurrealDB (or pauses the SurrealDB container) and asserts the API comes up once the DB does.

### CORS from config

`CorsLayer::permissive()` is a dev convenience, not a production posture.

- `CORS_ALLOWED_ORIGINS`: comma-separated **exact** origins — scheme + host (+ port), no trailing slash, no wildcards, e.g. `https://polymix.fly.dev,https://app.polymix.example`.
- Resolution at startup: variable set → `AllowOrigin::list` with exactly those origins; unset + `AUTH_DEV_MODE=true` → permissive (unchanged local DX); unset + non-dev → **fail startup** with a clear message. A prod deploy must never silently run permissive.
- Allowed methods `GET, POST, PUT, DELETE`; allowed headers `authorization, content-type`; `max_age` 300 s so preflights are cached.
- `/api/ws` is unaffected: browsers do not enforce CORS on WebSocket upgrades — its protection is the JWT in the query string, nothing else. Don't pretend otherwise with WS-specific CORS code.

### Readiness endpoint

- `/api/health` stays exactly as is: liveness, no auth, **no dependencies**. Never wire the DB into liveness — a DB outage would turn into an API restart loop.
- New `GET /api/ready`, no auth: runs `RETURN 1` on the system session wrapped in a 1 s `tokio::time::timeout`. Success → `200 {"status":"ready"}`; timeout or error → `503 {"status":"unavailable"}` with the real error logged at `error` (not leaked in the body).
- Point the Fly.io HTTP service checks at `/api/ready`; liveness-style restarts (if configured at all) at `/api/health`. Same split applies to any future compose healthcheck on the api container.
- Test: readiness returns 503 while the SurrealDB testcontainer is paused, 200 after it resumes.

## Backend conventions

- Rust edition 2024. Key crates (pin at these majors): `axum` 0.8, `tokio` 1, `surrealdb` 3 (**not 2 — see the tenant-session section below**), `serde`/`serde_json` 1, `jsonwebtoken` 9, `tower-http` 0.6 (CORS, trace), `tracing` + `tracing-subscriber`, `thiserror` (domain), `anyhow` (binaries only), `ulid` 1, `validator` 0.20, `testcontainers` for integration tests, `fake` + `rand` in the seeder.
- Layering: handler (extract, call service, map error) → service in `domain` (validation, numbering, transitions, totals) → repository trait in `domain`, implemented in `surreal-store`. Handlers contain no business logic; `domain` has no SurrealDB types.
- Repository trait shape (one per entity):

```rust
#[async_trait]
pub trait CustomerRepo: Send + Sync {
    async fn list(&self, q: ListQuery) -> Result<Paged<Customer>, RepoError>;
    async fn get(&self, id: &Id) -> Result<Option<Customer>, RepoError>;
    async fn create(&self, data: NewCustomer) -> Result<Customer, RepoError>;
    async fn update(&self, id: &Id, data: NewCustomer) -> Result<Customer, RepoError>;
    async fn delete(&self, id: &Id) -> Result<(), RepoError>;
}
```

- Repos are constructed per request for the request's tenant db (a `Store` factory holds the SurrealDB client; `store.for_tenant(&auth.tenant_db)` yields the repos). All SurrealDB queries use bound parameters — never format user input into query strings.

### SurrealDB connections & tenant sessions

This is version-sensitive; get it wrong and requests leak across tenants.

- **SDK 2.x behavior (the trap):** `Surreal<C>::clone()` is a cheap clone of the *same session* — `use_ns()`/`use_db()` on any clone switches every clone. A shared client with per-request `use_db` is a cross-tenant data race. Do not use SDK 2.
- **SDK 3.x behavior (what we use):** since 3.0, `clone()` creates a **new session with independent state** (namespace/database selection, auth, session variables, transactions) on the same underlying connection. The clone inherits the current session state and diverges from there. This is the SDK's documented multi-tenancy mechanism: <https://surrealdb.com/docs/languages/rust/concepts/multi-tenancy>. SDK 3.1+ supports SurrealDB servers v2.0–v3.2.

The `Store` in `surreal-store`:

```rust
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

pub struct Store {
    // Root-authenticated session pinned to the `system` db. Never handed out
    // directly — tenant sessions are cloned from it.
    root: Surreal<Client>,
    ns: String,
}

impl Store {
    pub async fn connect(cfg: &DbConfig) -> surrealdb::Result<Self> {
        let db = Surreal::new::<Ws>(cfg.url.as_str()).await?;
        db.signin(Root { username: &cfg.user, password: &cfg.pass }).await?;
        db.use_ns(&cfg.ns).use_db("system").await?;
        Ok(Self { root: db, ns: cfg.ns.clone() })
    }

    /// Session for the shared `system` db (tenant registry).
    pub fn system(&self) -> Surreal<Client> {
        self.root.clone()
    }

    /// Independent session for one tenant db, per request. SDK >= 3.0 only:
    /// the clone gets its own session, so this use_db cannot affect any
    /// other in-flight request.
    pub async fn for_tenant(&self, tenant_db: &str) -> surrealdb::Result<Surreal<Client>> {
        let session = self.root.clone();
        session.use_ns(&self.ns).use_db(tenant_db).await?;
        Ok(session)
    }
}
```

Rules that follow from this:

- One `Store::connect` at startup; `Store` lives in Axum state. Handlers call `store.for_tenant(&auth.tenant_db)` and pass the returned session to the repos for that request. Sessions are cheap; do not cache them per tenant.
- The WS hub takes its own long-lived session per tenant the same way (`for_tenant` at live-query setup), so live queries and request traffic never share a session.
- Never call `use_ns`/`use_db` anywhere except inside `Store` — grep-able invariant, enforce in review.
- If the SDK must ever be downgraded to 2.x (it shouldn't), the entire pattern changes: clones share sessions there, so it becomes one *connection* per tenant cached in a `RwLock<HashMap<String, Surreal<Client>>>`. Record such a change as an ADR.
- Domain errors: `NotFound`, `Validation(map)`, `Conflict(msg)`, `Store(source)` — mapped to the API error envelope in one `IntoResponse` impl.

## Frontend conventions

- Key deps (majors): `react` 19, `@mantine/core`+`@mantine/hooks`+`@mantine/form` 8, `@tanstack/react-query` 5, `@tanstack/react-router` 1, `@tanstack/react-table` 8, `i18next` + `react-i18next` + `i18next-icu`, `zod` 3, `@clerk/clerk-react` 5.
- Routes: `/` (redirect to `/customers`), `/customers`, `/customers/new`, `/customers/$id`, and the same trio for `orders` and `invoices`. Detail routes show a view/edit form. Global omnibox is a `Ctrl+K` Mantine `Spotlight`-style overlay, not a route.
- Each `features/<entity>` folder: `api.ts` (typed fetchers + query keys), `List.tsx` (TanStack Table: sortable columns, server pagination), `Detail.tsx`, `Form.tsx` (Mantine form + zod schema shared between create/edit), `types.ts` (zod schemas are the source of truth; infer TS types from them).
- `lib/api`: single `fetchJson` wrapper — injects the bearer token (from Clerk's `getToken()` or the dev token), parses the error envelope into a typed `ApiError`, used by all features.
- `lib/money`: `formatMoney(money, locale)` via `Intl.NumberFormat(locale, {style:'currency', currency})` from `amount_minor` (derive the minor-unit divisor from `Intl` resolved options — do not hardcode 100; JPY has 0 decimals). Form inputs edit decimal strings and convert to/from minor units at the boundary.
- i18n: namespaces `common`, `customers`, `orders`, `invoices`, `search`; files at `src/lib/i18n/locales/{en,de}/<ns>.json`. No literal user-facing strings in components — everything through `t()`. Language switcher in the app header; choice persisted to localStorage.
- Query keys: `['customers']` + params for lists, `['customers', id]` for details — consistent across features because the WS layer invalidates by these keys.

## Commands

A `justfile` at the repo root is the single entry point (CI calls the same recipes):

```
just dev          # compose up surrealdb + cargo run api (dev mode) + vite dev
just check        # fmt --check, clippy -D warnings, cargo test, eslint, tsc --noEmit, vitest
just test-int     # backend integration tests (testcontainers)
just seed         # seeder against local dev tenant
just build        # docker build all images
```

---

## Milestones

Each milestone ends runnable and demoable, with explicit acceptance criteria.

1. **M0 — Skeleton (walking skeleton).**
   Compose file with SurrealDB v3; Cargo workspace with the three crates; Axum app with `/api/health`, config loading, tracing, JWT middleware (JWKS-based) and the dev issuer; Vite/Mantine shell with router, app frame (header, nav, content), i18next initialized with `en` only; `justfile`; CI running `just check` + docker builds.
   **Done when:** `just dev` serves frontend + API; `curl /api/health` → ok; a request without a token → 401 envelope; `POST /dev/token` then `GET /api/me` → 200 with the dev org auto-provisioned as a tenant (registry record + tenant db exist); `just check` green in CI.

2. **M1 — Tenancy + Customers CRUD.**
   Clerk integration behind `VITE_AUTH_MODE` (`<SignIn/>`, `<OrganizationSwitcher/>`); tenant registry + provisioning as specced; customer entity end-to-end: repo, service with validation, five REST routes, list page (TanStack Table, server pagination + sorting), detail, create/edit forms (Mantine + zod).
   **Done when:** with two dev orgs, customers created in one are invisible in the other (this is the tenant-isolation integration test — mandatory, running via the harness in **Integration test harness + CI**); all customer CRUD works from the UI; validation errors from the API render on the matching form fields; Clerk mode works in a browser against a real Clerk app; `just test-int` runs green in CI as its own job.

3. **M2 — Orders & Invoices.**
   Order and invoice entities per the data model: numbering, status transitions, invoice-from-order, totals math in the service; UI lists (orders filterable by customer and status), forms with line-item editing (add/remove rows), status-transition buttons; seeder crate producing ≥10k customers / ≥100k orders per demo tenant (batched inserts of 1000).
   **Done when:** full flow in the UI — create order for a customer, confirm it, generate the invoice, issue it; invalid transitions rejected with 409 and a visible error; totals (net/tax/gross) correct in unit tests including rounding cases; `just seed` completes and the customers list stays responsive (<1 s page loads) on the seeded tenant.

4. **M3 — Full-text search.**
   Migration adding analyzer + the three FTS indexes; `q` param on list endpoints; `/api/search` omnibox endpoint; `Ctrl+K` overlay with keyboard navigation and highlighted matches; debounced (250 ms) search-as-you-type.
   **Done when:** searching a customer name prefix ("ada" finds "Adamant Print GmbH") works in lists and omnibox; omnibox navigates to the selected record on Enter; FTS integration test asserts ranking (exact-prefix beats mid-word); p95 < 100 ms for the search endpoint on the seeded volume (measure with a quick script, record in `/docs/perf.md`).

5. **M4 — i18n + currency + org settings.**
   `ua` locale files for every namespace; language switcher; all dates/numbers locale-formatted; invoice in a non-default currency with rate snapshot from the seeded `exchange_rate` table; display-only converted amount ("≈ UAH 1,234.56") on the invoice detail; Order and Invoice table prefixes are configured at organization level - default is empty so no prefix displayed, just number; admin prefix edit out-of-scope; create and seed database 100 customers, 1000 orders with ukrainian names.
   **Done when:** switching to `ua` translates every screen (no raw keys, no leftover English in the main flows) and reformats dates/numbers; an invoice created in USD on a EUR tenant stores the rate snapshot and renders both amounts; money round-trips through forms without losing cents (unit tests on the minor-units conversion).

6. **M4.1 — Order screen: customer & currency selection.**
   Order create/edit form gets an explicit **customer selector** (searchable Mantine `Select`/autocomplete backed by the customer FTS from M3 — type-ahead, shows name, resolves to `customer_id`; required, validated) and a **currency selector** (ISO 4217 options; defaults to the tenant default currency, editable per order; line-item unit prices and the computed total render in the chosen currency). No new backend fields — `order.customer_id` and `order.currency` already exist in the data model; this wires the UI to set them deliberately instead of relying on defaults.
   **Done when:** creating an order lets the user search and pick a customer (no manual id entry) and pick a currency other than the tenant default; the selected currency drives money formatting on the form and the saved order; validation rejects an order with no customer selected; existing order CRUD and totals tests stay green.

7. **M5 — Live updates.**
   Hub + live queries per the WS spec; frontend WS client with reconnect + invalidate-on-reconnect; optimistic updates on edit/transition mutations. Build order, module layout, and per-step tests: see **M5 work breakdown (Live updates)** above.
   **Done when:** two browsers on one tenant — an edit in one appears in the other within 1 s without user action, on both list and detail views; a browser on a second tenant sees nothing (integration test: WS client of tenant B receives no event for tenant A's mutation — mandatory); killing and restarting SurrealDB recovers live updates without a page reload.

8. **M5.1 — Customer CRM profile (Ukraine-focused).**
   Extend the customer entity into a CRM-grade profile: kind (юр. особа / ФОП / фіз. особа), legal identification (ЄДРПОУ, РНОКПП, ІПН ПДВ, IBAN), lifecycle status (`lead → active ↔ inactive`, `blocked`) with a status-transition route and dictionary endpoint, embedded contacts array, legal + delivery addresses, tags/industry/source, and commercial terms (payment terms, credit limit, default currency/discount); wider customer FTS index; migration of legacy `contact_name`/`email`/`phone`/`address` fields. No customer numbering — unlike orders/invoices, a customer is not a document with an external reference to number, and an initial attempt at one (`CUS-000123`-style, tenant `customer_prefix`) was dropped as scope that didn't belong (`docs/adr/0011-drop-customer-numbering.md`). Spec, data model, and step-by-step build order: `docs/customers-crm.md` (normative).
   **Done when:** the acceptance pass and perf re-check in `docs/customers-crm.md` Step 6 pass — extended CRUD end-to-end from the UI in `ua` locale, migration of pre-M5.1 rows verified by integration test, order creation guarded by customer status (lead auto-promote, blocked → 409), omnibox finds customers by ЄДРПОУ and contact name, search p95 still < 100 ms on the seeded volume.

9. **M6 — Cloud + perf pass.**
   Dockerfiles (multi-stage; frontend served by nginx); fly.toml for api + SurrealDB (volume-backed) + static frontend; the three items in **Operational hardening (M6)** — startup retry, CORS from config, readiness endpoint; k6 scripts for search, list pagination, and mutation fan-out with 100 concurrent WS clients; numbers recorded in `/docs/perf.md` against the NFR targets.
   **Done when:** the demo runs on Fly.io URLs end-to-end including Clerk sign-in; starting the API before SurrealDB recovers without manual intervention (integration test); `/api/ready` flips 503 ↔ 200 as the DB goes down/up (integration test); a non-dev start without `CORS_ALLOWED_ORIGINS` fails with a clear error, and a preflight from an unlisted origin gets no `Access-Control-Allow-Origin` header; k6 runs are scripted (`/deploy/k6/`), repeatable, and their p95s are written to `/docs/perf.md` with a pass/fail verdict per NFR.

10. **M7 — Customer portal + instant quote** (post-prototype candidate): public product configurator with a parametric pricing engine. This is the first item of the post-prototype roadmap below; its design is already written (`docs/instant-quote.md`, `docs/product-configuration.md`, and the normative `docs/quote-engine-spec.md`).

## Testing

- **Backend:** unit tests in `domain` (validation, totals + rounding, status transitions, numbering); integration tests in `surreal-store`/`api` against a real SurrealDB via testcontainers — CRUD, FTS ranking, live-query delivery, and tenant isolation (the isolation test is mandatory, not optional). Integration tests use the dev issuer for tokens.
- **Frontend:** Vitest + Testing Library for forms and money/i18n utilities; one Playwright smoke covering login (dev mode) → create customer → search finds it → live update visible in a second context.

### Integration test harness + CI (lands with M1)

The first integration test (tenant isolation) arrives in M1, so the harness and the CI wiring land with it — not retrofitted later.

**Test conventions**

- Integration tests are `#[tokio::test]` + `#[ignore]` in `tests/` directories of `surreal-store` and `api`. `cargo test --workspace` (inside `just check`) therefore stays DB-free and fast; `just test-int` (`cargo test --workspace -- --ignored`) runs the real thing. Never gate on an env var instead of `#[ignore]` — a skipped-by-default test that silently never runs anywhere is the failure mode to avoid.
- **One SurrealDB container per test binary, not per test.** Start `surrealdb/surrealdb:v3.2.1` (same tag as compose — keep them in lockstep) once via testcontainers behind a `tokio::sync::OnceCell`; every test gets the shared connection.
- **Isolation between tests comes from the db-per-tenant design itself:** each test provisions its own tenant(s) with a unique org id (`test_<ulid>`), which yields a fresh database. No truncation, no ordering dependencies, tests run in parallel. The `system` db is shared — tests must not assert on global registry counts, only on rows they created.
- `api`-level tests boot the real router (dev issuer enabled) via `axum_test`/`tower::ServiceExt::oneshot` against the shared container, mint tokens from the dev issuer, and exercise real HTTP — the tenant-isolation test creates a customer as org A and asserts org B's list is empty and org B's `GET /api/customers/{id}` is 404 **through the API**, not the store.

**CI wiring (`.github/workflows/ci.yml`)**

- New `test-int` job, parallel to `check` (not chained after it — wall-clock matters more than saving a cache miss): checkout, Rust toolchain, `Swatinem/rust-cache` (same `workspaces: backend` key so the build cache is shared), `extractions/setup-just`, `just test-int`. Docker is preinstalled on `ubuntu-latest`; testcontainers needs zero extra setup there.
- Add `timeout-minutes: 20` on the job (testcontainers hangs manifest as stuck jobs, not failures — bound them) and a top-level concurrency block so stale pushes stop burning runners:

  ```yaml
  concurrency:
    group: ${{ github.workflow }}-${{ github.ref }}
    cancel-in-progress: true
  ```

- The `build` job's `needs: check` should become `needs: [check, test-int]` — images only build once both gates pass.
- Local note (Windows/podman): testcontainers talks to the Docker socket. With podman, expose it via `podman machine` socket + `DOCKER_HOST`; document the one-liner in the readme when it first bites, don't build tooling around it.

## Risks

| Risk | Mitigation |
|---|---|
| SurrealDB maturity (prototype-fine, production-unknown) | Repository trait isolates it; ADR records Postgres + Meilisearch as the fallback stack; M6 perf numbers decide |
| Live query behavior under reconnect/scale | Hub owns reconnect + resubscribe; client refetches on WS reconnect so a missed event never leaves stale UI |
| FTS lacks typo tolerance | Edge-ngram prefix search; if search quality disappoints, Meilisearch sidecar is an additive change behind the same endpoint |
| db-per-tenant count explosion | Non-issue at prototype scale; note in ADR that production may shard tenants across nodes |

## Out of scope (prototype)

Estimating/quoting engine, production scheduling/job board, shipping, custom user-management UI (Clerk provides sign-in, invitations, and org membership), RBAC beyond tenant isolation, offline support, reporting/BI, PATCH endpoints, soft deletes, audit log, multi-currency accounting beyond the display-only rate snapshot.

Everything in this list has a home in the post-prototype roadmap below — "out of scope" means *deferred*, not *rejected*.

---

# Post-prototype roadmap

What we do once the prototype **appears successful**. Nothing here starts before the gate below is passed and recorded; everything here assumes the prototype's architecture survives the gate (possibly with the stack swap the gate allows).

## Gate — declaring the prototype successful

The prototype is judged after M6, against evidence, and the verdict is written as an ADR (`docs/adr/`, "prototype verdict"). It must answer three questions explicitly:

1. **Does SurrealDB stay?** Inputs: M6 k6 numbers vs the NFR targets (search p95 < 100 ms on seeded volume; list pagination p95 < 200 ms; live-update fan-out stable at 100 concurrent WS clients), operational behavior observed during M5/M6 (live-query reconnect, memory under load, backup story), and developer friction encountered. Outcomes: **keep** (proceed as-is), or **swap** to the recorded fallback (Postgres + Meilisearch + LISTEN/NOTIFY-or-CDC behind the same repository traits and hub) as the *first* post-prototype workstream — the traits and the WS protocol were designed so this swap does not ripple past `surreal-store` and the hub internals.
2. **Does Clerk stay?** Inputs: per-MAU cost projected at target tenant count, EU data-residency requirements of the DACH launch market, and any friction with org-per-tenant. The JWT middleware was built provider-agnostic precisely so the answer can be "swap to Zitadel/Logto" without touching the backend beyond config.
3. **Is the demo convincing to real users?** At least two demo sessions with actual print-shop staff (not just us) using the seeded tenant; their reactions to search, live updates, and the order→invoice flow are recorded in the ADR. If the core loop doesn't land with them, we fix that before building anything new.

Passing the gate does **not** mean the prototype code is production code. It means the architecture is confirmed; each track below states what gets hardened vs rebuilt.

## Track A — Product depth (the business)

Ordered by dependency; A1 and A2 are the headline features that justified "Next".

1. **A1 — Quote engine v1** (`crates/quote-engine`). Implement `docs/quote-engine-spec.md` exactly — it is already written to be executed mechanically (integer micro-unit money, `PriceModel` schema, effect resolution, rule AST, golden fixture in §9). Pure crate, no I/O, property-tested (unit price monotonically non-increasing in quantity; every valid config prices without error), golden dataset must reproduce byte-identical ladders. This can start **in parallel with the gate** — it depends on nothing prototype-specific.
2. **A2 — Admin: price-model & product-template maintenance.** The tenant-facing UI for `material`, `machine`/`technology`, `operation`, `pricing_policy`, `format`, `product_template` + `option_effect` + `compatibility_rule` (per `docs/product-configuration.md`). Includes the **template linter** (completeness check: every reachable option combination yields a priceable JobSpec) run on save, and `pricelist_version` stamping on publish. This is the real MIS work — the engine is only as good as the data a tenant can maintain. Work breakdown (split into a catalog slice that unblocks staff quoting, and the template editor): `docs/pricing-admin-plan.md`.
3. **A3 — Customer portal (public instant quote).** Public, unauthenticated configurator per tenant (tenant resolved by subdomain/slug); renders a `product_template`, greys out options via `compatibility_rule`, shows the price ladder live (<300 ms per parameter change, server-computed), lets the customer pick a quantity and submit a quote request with contact details + artwork upload placeholder. Quote is stored with its `pricelist_version`.
4. **A4 — Quote → order pipeline.** Staff view of incoming quote requests; accepting one creates an `order` carrying the engine's production plan (imposition, technology choice, operations) — the plan is what production scheduling (A6) consumes. Internal estimating is designed as its own feature in `docs/staff-quoting.md` (direct-JobSpec estimating beyond templates, quote documents with lifecycle and quote→order conversion, audited margin/discount/price overrides) — it can ship before A2/A3 and subsumes this item's estimating clause.
5. **A5 — Documents & email.** Invoice **PDF generation** (legally required to invoice anyone; Typst or headless-Chromium rendering — decide by ADR) with correct VAT presentation for DE, quote PDFs, and transactional email (invoice/quote delivery, quote-request confirmation). High priority: without A5 no tenant can actually run their business on the system.
6. **A6 — Production scheduling / job board.** Orders' production plans become jobs on a board (queued → on-machine → done per operation), machine calendars, drag-reordering, the shop-floor screen. Live updates (M5 hub) are the transport — this is where they pay off beyond demos.
7. **A7 — Materials & inventory.** Stock levels on `material`, decrement from completed production plans, reorder alerts. Feeds real material costs back into the price model.
8. **A8 — Reporting/BI.** Revenue by customer/product/period, margin per order (engine cost vs invoiced price), quote conversion rate. Start as read-only SQL/SurrealQL views + CSV export; a real BI story is its own later decision.

## Track B — Platform hardening (prototype code → production code)

Runs alongside Track A; B1–B4 are prerequisites for onboarding the **first paying tenant**, the rest can follow.

1. **B1 — RBAC.** Roles `admin`, `sales`, `production`, `finance` per tenant, carried as a claim (Clerk org roles or the replacement provider), enforced in the API layer per route + in the UI (hide what you can't do). Tenant isolation stays the hard boundary; RBAC is authorization *within* a tenant.
2. **B2 — Data safety.** Automated backups with tested restore (restore drill is part of "done"), point-in-time recovery story for the chosen DB; invoice immutability + audit log (who changed what, when — required for GoBD compliance in the German market); soft-delete semantics where legal retention requires them.
3. **B3 — API robustness.** Optimistic concurrency (`version` field, `409` on stale write — the prototype's last-write-wins is not acceptable with multiple staff), idempotency keys on POST mutations, rate limiting per tenant, request size limits, PATCH where full PUT is impractical (line-item-heavy orders).
4. **B4 — Observability & environments.** OTLP traces/metrics/logs to a real backend, alerting on error rate + p95s + WS-hub health; `dev`/`staging`/`prod` environments with promotion via CI/CD; secrets in a manager, not env files. Fly.io stays until scale forces the move — images were the portability bet.
5. **B5 — Migration discipline at fleet scale.** Per-tenant migrations must run across *N* tenant databases reliably: ordered, resumable, with a fleet-wide status view. This is the operational cost of db-per-tenant — pay it deliberately.
6. **B6 — Load-shedding & tenant fairness.** Per-tenant quotas on WS connections and query cost so one tenant's traffic can't starve another (single shared API + DB node until sharding is warranted; the ADR from the prototype already notes sharding tenants across nodes as the growth path).

## Track C — Commercialization

1. **C1 — Tenant onboarding.** Self-service signup: create org → auto-provision tenant (already built) → guided setup wizard (currency, VAT rate, first materials/machines from a starter price-model template per country). A starter `PriceModel` matters — an empty configurator sells nothing.
2. **C2 — Legacy PolyMix import.** Tooling to migrate existing PolyMix customers' data (customers, open orders, invoice history, price lists) — the installed base is the first market. Format analysis of the legacy store is its own spike; budget it early because it de-risks every sales conversation.
3. **C3 — Billing.** Subscription billing (Stripe), plan limits (users, quote volume), dunning. Keep entitlements as claims/tenant-registry flags so the API enforces them in one place.
4. **C4 — Compliance & trust.** GDPR (DPA, data export, deletion workflow), GoBD statement for DE, uptime page. Needed before mid-size shops sign.

## Suggested sequence

The first three moves after the gate, in order:

| Step | What | Why first |
|---|---|---|
| 1 | A1 quote engine (can start pre-gate) + B1 RBAC + B2 backups/audit | Engine is the differentiator and has zero coupling to the gate outcome; B1/B2 block any real tenant |
| 2 | A5 documents/email + B3 API robustness | Makes the existing CRUD core actually usable to run a shop |
| 3 | A2 admin price-model UI → A3 portal → A4 pipeline | The instant-quote story, shipped in customer-visible slices |

Track C starts when step 2 is done (nothing to onboard tenants *onto* before that); A6–A8 follow demand from the first real tenants rather than a fixed order.
