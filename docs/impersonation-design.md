# Design: Application-level user impersonation

## Status

Proposed — not yet implemented. To be built **after** RBAC
(`docs/rbac-design.md`): the env allowlist described below is superseded
by the `platform:impersonate` permission of the platform-admin plane; the
rest of this design is unchanged.

## Context

Support and admin staff need to see the application exactly as a given
customer sees it ("sign in as user X in org Y") to diagnose issues. Clerk
offers this natively via actor tokens, but the free plan caps it at 5
impersonations per month; more requires the paid Enhanced Administration
add-on.

We can implement the feature entirely in our own system because we already
own the whole identity path:

- The backend verifies the Clerk JWT itself (`crates/api/src/auth.rs`) —
  Clerk is only a token issuer to us, not an enforcement point.
- Effective identity is just three claims: `sub`, `org_id` (configurable via
  `AUTH_ORG_CLAIM`), and `org_name`. Everything downstream —
  `AuthContext`, tenant DB resolution, repos — derives from them.
- We already have infrastructure for minting our own RS256 tokens
  (`DevIssuer` in `crates/api/src/dev_issuer.rs`, currently dev-mode-only),
  and `validate_token` already resolves keys by `kid` through a JWKS cache.

So impersonation = "mint a second, internally-signed token whose identity
claims describe the target, plus an `act` claim recording who is really
driving" — the same model Clerk's own actor tokens use (RFC 8693 `act`
claim).

## Goals / non-goals

**Goals**

- An authorized staff member can obtain a time-limited session as any user
  of any org, without involving Clerk.
- The real actor is preserved end to end: in the token, in `AuthContext`,
  in logs, and in a persistent audit record.
- The impersonated session behaves identically to a real one (REST + WebSocket
  live updates), so what staff sees is what the user sees.
- Clear UI indication that impersonation is active, with one-click exit.

**Non-goals (for the first iteration)**

- General RBAC. Roles are deferred work (PLAN.md B1); until then,
  impersonation rights come from a config allowlist.
- Server-side revocation of individual impersonation sessions (mitigated by
  short TTL; see Extensions).
- Impersonating in Clerk's own UI/components — the impersonation exists only
  in our app.

## Design overview

```
Admin (real Clerk token)                         Backend
        │                                           │
        │ POST /api/impersonation                   │ 1. validate Clerk token
        │   { target_org_id, target_user_id }       │ 2. caller in allowlist?
        │──────────────────────────────────────────▶│ 3. caller not already impersonating?
        │                                           │ 4. audit event → system DB
        │   { token, expires_at }                   │ 5. mint internal JWT:
        │◀──────────────────────────────────────────│    sub=target_user, org_id=target_org,
        │                                           │    act={sub: admin}, exp=+30min, jti
        │ subsequent requests use that token         │
        │ (REST Authorization header, WS ?token=)   │ validate_token resolves the internal
        │                                           │ kid, enforces internal issuer, and
        │                                           │ surfaces actor in AuthContext
```

The impersonation token flows through the exact same channels as a Clerk
token, so **no handler, repo, or WS code changes** — only the auth layer
and one new route.

## Backend changes

### 1. `InternalIssuer` (generalize `DevIssuer`)

Promote the existing `DevIssuer` into an `InternalIssuer` that is
constructed whenever impersonation is enabled (not only in dev mode):

- RSA keypair generated at startup, `kid = "internal-<random>"` — distinct
  from Clerk's kids and from `dev-key-1`.
- Issuer string: a dedicated internal issuer value (e.g.
  `polymix-internal`), configured separately from `AUTH_ISSUER` so a forged
  "internal-looking" token can never pass the Clerk validation path and
  vice versa.
- Key is process-local and never persisted. A backend restart invalidates
  outstanding impersonation sessions — acceptable given the short TTL, and
  it doubles as a global kill switch.

Dev mode keeps working: `DevIssuer` becomes a thin wrapper or second
instance of the same type mounted at `/dev/*` as today.

### 2. Token shape

```json
{
  "iss": "polymix-internal",
  "sub": "<target_user_id>",
  "org_id": "<target_org_id>",
  "org_name": "<from tenant registry>",
  "act": { "sub": "<admin_user_id>" },
  "jti": "<uuid>",
  "iat": ..., "exp": "iat + 30 min"
}
```

`org_name` is copied from the tenant registry row (`tenant.name`) so
`ensure_tenant` doesn't overwrite the org's display name with a stale
value.

### 3. `validate_token` — accept two trust roots

In `crates/api/src/auth.rs`, key resolution branches on `kid`:

- `kid` matches the internal issuer's key → validate with the internal
  public key, require `iss == internal issuer`, and read `act.sub` into
  the actor field.
- otherwise → existing Clerk JWKS path, unchanged; a Clerk token carrying
  an `act` claim is rejected (defense against Clerk-side actor tokens
  slipping through with semantics we don't audit).

### 4. `AuthContext` gains the actor

`crates/domain/src/auth.rs`:

```rust
pub struct AuthContext {
    pub user_id: String,
    pub org_id: String,
    pub tenant_db: String,
    /// Real user driving the session when impersonating; None otherwise.
    pub actor_user_id: Option<String>,
}
```

Handlers keep working untouched (they act on `user_id`/`tenant_db` as
before). `tracing` request spans include `actor_user_id` when present so
every impersonated request is attributable in logs.

### 5. New route: `POST /api/impersonation`

Mounted inside the protected subtree (so the caller's real Clerk token is
validated by `require_auth` first).

Request `{ "target_org_id": "...", "target_user_id": "..." }` →
response `{ "token": "...", "expires_at": "..." }`.

Authorization checks, in order:

1. Feature enabled (`IMPERSONATION_ENABLED=true`).
2. Caller's `user_id` is in `IMPERSONATOR_USER_IDS` (comma-separated Clerk
   user ids). 403 otherwise.
3. Caller is not themselves impersonating (`actor_user_id.is_none()`) —
   no chaining.
4. `target_org_id` exists in the tenant registry — impersonation must not
   auto-provision new tenants. 404 otherwise.

Then: write the audit event, mint, return. `target_user_id` is free-form
(we don't store per-org user lists; identity of users within a tenant is
whatever Clerk says), so the endpoint takes it verbatim — the audit trail
is the guardrail.

### 6. Audit log

New `impersonation_event` table in the shared `system` DB (same pattern as
the `tenant` registry — defined idempotently in `Store::connect`):

```
{ id, actor_user_id, target_user_id, target_org_id, jti, issued_at, expires_at }
```

One row per session start. Combined with `actor_user_id` on request spans,
this answers both "who impersonated whom, when" and "what did they do".

### 7. `/api/me` reports impersonation

Extend the response with `actor_user_id` (nullable) and, for the admin UI,
`can_impersonate: bool`. The frontend uses these to render the banner and
to gate the impersonation controls.

### Config additions (`crates/api/src/config.rs`)

| Env var | Default | Meaning |
| --- | --- | --- |
| `IMPERSONATION_ENABLED` | `false` | Master switch; endpoint 404s when off |
| `IMPERSONATOR_USER_IDS` | empty | Comma-separated Clerk user ids allowed to impersonate |
| `IMPERSONATION_TTL_SECS` | `1800` | Token lifetime |

## Frontend changes

All frontend auth flows through the app's own `AuthContext`
(`lib/auth/context.ts`) and the single `getToken` seam in
`fetchJson.ts` / `WsClient.ts`, which makes this small:

1. **Impersonation state** — a new context layered inside `AuthProvider`
   holding `{ token, targetUserId, targetOrgId, expiresAt } | null`, kept
   in `sessionStorage` (survives reload, not shared across tabs, gone on
   browser close).
2. **`getToken` override** — while impersonation is active, `getToken`
   returns the impersonation token instead of Clerk's; `orgId` reports the
   target org. Because `LiveUpdatesProvider` keys the WS connection by
   `orgId` and re-fetches the token on connect, entering/leaving
   impersonation automatically tears down and reopens the live stream as
   the right tenant. React Query caches are cleared on both transitions.
3. **Start UI** — an "Impersonate" action (org picker fed by a small
   admin-only endpoint or manual org-id entry for v1), rendered only when
   `/api/me` returns `can_impersonate: true`.
4. **Banner** — a persistent, visually loud bar: "Viewing as {user} in
   {org} — expires {t} — [Stop]". Stop discards the stored token and
   invalidates caches. On a 401 (token expired), the impersonation state
   is dropped and the admin falls back to their own session instead of
   being signed out.

Clerk's `<OrganizationSwitcher>` stays wired to the real Clerk session;
while impersonating it is hidden to avoid two competing notions of "current
org".

## Security considerations

- **No privilege escalation surface for regular users**: the endpoint
  requires a validated Clerk token *and* allowlist membership; the
  allowlist lives in deployment config, not in data a tenant can touch.
- **No chaining**: impersonation tokens are rejected by the impersonation
  endpoint, so a stolen impersonation token cannot mint further tokens.
- **Blast radius of a leaked token**: one tenant, ≤30 minutes, fully
  audited, and globally revocable by restarting the API (rotates the
  in-memory key).
- **Trust-root separation**: distinct issuer strings and kid namespaces
  mean neither validation path can accept the other's tokens.
- **Dev mode unchanged**: `POST /dev/token` remains dev-only; production
  impersonation never goes through it.
- **Writes are allowed** while impersonating (that's the point of
  reproducing user issues), relying on the audit trail. A `read_only` flag
  in the token is a cheap later addition if policy demands it.

## Testing

- Unit: internal-token validation (happy path, wrong issuer, expired,
  Clerk token with `act` rejected, chaining rejected).
- Integration (existing testcontainers harness): allowlisted admin mints a
  token → requests hit the *target* tenant DB and not the admin's; audit
  row written; non-allowlisted caller gets 403; unknown org gets 404;
  WS connect with impersonation token joins the target tenant's hub.

## Extensions (out of scope for v1)

- **Revocation**: store `jti` per session and check a denylist in
  `validate_token`; enables "terminate session" from an admin panel.
- **RBAC integration**: when PLAN.md B1 lands, replace the env allowlist
  with a `superadmin` role claim.
- **Read-only impersonation** mode.
- **Per-tenant consent/visibility**: show tenant admins a log of support
  impersonation sessions into their org.
