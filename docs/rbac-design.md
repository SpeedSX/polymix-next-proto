# Design: RBAC — platform admins and tenant roles

## Status

Proposed — not yet implemented. Corresponds to roadmap item **B1** in
PLAN.md. Prerequisite for `docs/impersonation-design.md` (impersonation
rights become a platform-admin permission instead of an env allowlist).

## Context

Two distinct kinds of privileged access are needed:

1. **Platform administrators** — our own staff. Cross-tenant: manage
   tenants, help with onboarding, troubleshoot (including impersonation).
   Not members of any customer org.
2. **Tenant roles** — business users *within* one tenant: directors,
   managers, sales, production, finance staff. Authorization within a
   tenant; tenant isolation (one SurrealDB database per org) stays the
   hard boundary underneath.

PLAN.md B1 assumed roles would be "carried as a claim (Clerk org roles)".
That does not survive contact with Clerk's pricing: the free plan ships
only the default `admin` / `member` org roles — custom roles (up to 10)
require a paid plan for production use. Consequence, consistent with the
impersonation design: **our database is the source of truth for roles;
Clerk claims are used only to bootstrap.**

## Goals / non-goals

**Goals**

- Every authenticated request resolves to an effective permission set,
  enforced in the API layer per route and reflected in the UI (hide what
  you can't do).
- Tenant admins manage their own members' roles; platform admins are
  invisible to tenants and managed by us.
- New orgs and new members get sane roles without manual backend work
  (bootstrap from Clerk's free-tier `admin`/`member` org roles).
- Tenant admins can configure their own roles: rename, create, delete
  roles and edit each role's permission assignment. The roles below are
  the **default configuration** every new tenant starts from, not a fixed
  set.
- Impersonation (separate design) plugs in: `platform:impersonate` is a
  platform permission; an impersonated session gets exactly the target
  user's tenant permissions.

**Non-goals (v1)**

- Custom *permissions* — tenants compose roles from a fixed,
  code-defined permission catalog; they cannot invent new permission
  kinds (those only mean something if handlers check them).
- Per-user permission overrides — permissions attach to roles only.
- Object-level permissions ("only my own orders").
- A full platform admin console — v1 exposes only what onboarding and
  troubleshooting need.

## Model

### Two planes

| Plane | Scope | Storage | Managed by |
| --- | --- | --- | --- |
| Platform roles | cross-tenant | `platform_admin` table, shared `system` DB | us (CLI/seed; console later) |
| Tenant roles | one org | `member` table, per-tenant DB | tenant admins (members UI) |

A platform admin is identified by Clerk user id and is *not* a member of
customer orgs. A user could in principle appear in both planes; the planes
are evaluated independently.

### Permission catalog (fixed, in code)

Permissions are `resource:action` values, checked as a bitset. Verbs:
`read`, `write` (create/update/delete). Special actions get their own
permission where the business meaning differs from plain writes. The
catalog is a `domain`-crate enum — the fixed vocabulary handlers enforce
against; it grows only through code changes.

### Tenant roles (per-tenant data, seeded defaults)

Roles are rows in the tenant DB, each carrying a name and a set of
catalog permissions. Tenant admins can rename roles, edit their
permission assignment, and create or delete roles. Every new tenant is
seeded with this **default configuration**:

| Permission | admin | manager | sales | production | finance |
| --- | :-: | :-: | :-: | :-: | :-: |
| `customers:read` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `customers:write` | ✓ | ✓ | ✓ | | |
| `orders:read` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `orders:write` | ✓ | ✓ | ✓ | | |
| `orders:status` (production transitions) | ✓ | ✓ | | ✓ | |
| `invoices:read` | ✓ | ✓ | ✓ | | ✓ |
| `invoices:write` | ✓ | ✓ | | | ✓ |
| `invoices:issue` | ✓ | ✓ | | | ✓ |
| `settings:manage` (org prefixes, locale, currency) | ✓ | | | | |
| `members:manage` | ✓ | | | | |
| `roles:manage` | ✓ | | | | |

- **admin** — the "director" role: everything within the tenant. Unlike
  the others, `admin` is a **built-in** role: it always holds every
  catalog permission (including future ones), cannot be edited or
  deleted, and so guarantees a tenant can never lock itself out of its
  own role management.
- **manager** — day-to-day running of the shop, no org administration.
- **sales / production / finance** — PLAN.md B1's functional roles;
  "technical staff" maps to `production`. These (and any tenant-created
  roles) are fully editable.
- **pending** — implicit state, not a role row: a member with no role
  grants no permissions (see bootstrap). Every request returns 403 with a
  distinguishable code so the UI can show "awaiting role assignment".

Deleting a role requires it to have no members (409 otherwise — the UI
offers reassignment first). New catalog permissions added in later
releases default to unassigned for custom roles (admins opt in), while
`admin` picks them up automatically.

### Platform permissions (v1)

| Permission | platform_admin |
| --- | :-: |
| `platform:tenants:read` (list/inspect tenants) | ✓ |
| `platform:impersonate` | ✓ |

One platform role for now; the shape allows more (e.g. read-only support)
later.

## Storage

### `role` table (per-tenant DB, added to tenant migrations)

```
role: { id, key (unique index), name, permissions: array<string>,
        built_in: bool, created_at, updated_at }
```

Seeded with the five default roles on tenant provisioning; the same
seeding runs as a migration for existing tenants (startup already re-runs
migrations across all tenant DBs). `permissions` holds catalog values;
unknown values (e.g. after a permission is removed from the catalog) are
ignored on load rather than failing auth. `built_in` marks `admin`.

### `member` table (per-tenant DB, added to tenant migrations)

```
member: { id, user_id (unique index), role_key: option<string>,
          email?, display_name?, created_at, updated_at }
```

Both live in the tenant DB so tenant isolation covers role and
membership data too.
`email`/`display_name` are denormalized from the JWT when available,
purely for the members UI (we keep no cross-tenant user directory).

### `platform_admin` table (shared `system` DB)

```
platform_admin: { id, user_id (unique index), created_at }
```

Defined idempotently in `Store::connect` like the `tenant` registry.

### Creating a platform admin (v1 flow)

Platform admins are ordinary Clerk users of the same Clerk application —
there is no separate identity system. What makes them platform admins is
a row in `platform_admin`, and that table is **declaratively owned by
deployment config**:

1. **Staff org.** We keep one internal Clerk organization (e.g.
   "PolyMix Platform") that all staff belong to. This is required, not
   cosmetic: the auth path rejects tokens without an active org, and the
   provisioner needs an org to resolve. The staff org provisions a tenant
   DB like any other org; nothing tenant-side ever runs in it, it just
   satisfies the invariant that every session has an org. Staff sign in
   and select it like any org.
2. **Get the Clerk user id** of the new admin (Clerk dashboard → Users,
   or `clerk users list`).
3. **Add the id to `PLATFORM_ADMIN_USER_IDS`** (comma-separated) in the
   deployment's environment and restart/redeploy the API.
4. **Startup reconciliation** makes the table match the env var exactly:
   ids in the env but not the table are inserted, rows not in the env are
   deleted. Declarative rather than additive, so **revocation is the same
   flow** — remove the id, redeploy — and config review covers grants and
   revocations alike. The env var is the source of truth; there is no
   runtime mutation path in v1.
5. On the admin's next request (within the platform-admin cache TTL),
   the platform lookup matches, `/api/me` reports
   `platform_permissions`, and the platform UI (tenant list,
   impersonation entry point) appears.

When a platform console ships post-v1, ownership flips: the table becomes
runtime-managed (`platform:admins:manage`, with a last-admin guard like
the tenant plane) and the env var is demoted to first-boot seeding.

## Auth path changes

`authenticate_token` (crates/api/src/auth.rs) today produces
`AuthContext { user_id, org_id, tenant_db }`. It gains a **membership
resolution** step after `ensure_tenant`:

1. Look up `member` by `user_id` in the tenant DB, then the referenced
   `role` row (one moka cache keyed `(tenant_db, user_id)` holding the
   resolved permission set, short TTL ~30s — same pattern as the tenant
   cache; a member-role or role-permission change takes effect within the
   TTL, and the mutation endpoints additionally invalidate the tenant's
   entries so changes made through the UI apply immediately).
2. Member row missing → **bootstrap** (below). Role row missing or
   role-less member → pending (empty permission set).
3. `AuthContext` becomes:

```rust
pub struct AuthContext {
    pub user_id: String,
    pub org_id: String,
    pub tenant_db: String,
    pub role_key: Option<String>,        // None = pending
    pub permissions: PermissionSet,      // resolved from the role row
    pub platform: PlatformContext,       // platform permissions, usually empty
    pub actor_user_id: Option<String>,   // from impersonation design
}
```

Platform lookup happens only when the cheap check is warranted (cached
set of platform admin ids, refreshed periodically) — it's a lookup by
`user_id` in the system DB, independent of the org claim.

### Bootstrap: first sight of a user in an org

Clerk's free tier does give us one useful signal: the basic org role
(`admin` / `member`) can be added to the session token template (as an
`org_role` claim, next to the existing custom `org_name` claim). On a
missing member row:

- claim `org_role == "admin"` → create member row with tenant role
  **admin**. The org creator is a Clerk admin, so a fresh org's first
  user is immediately its director — no manual step.
- otherwise → create member row with **no role** (pending). A tenant
  admin assigns the real role in the members UI. The user sees an
  "awaiting role assignment" screen, not an error.

Bootstrap only ever *creates* rows; it never overwrites an assigned role
(so demoting a Clerk org admin in our system sticks even though the claim
still says `admin`).

## Enforcement

### API layer

A single helper, used at the top of each handler:

```rust
auth.require(Permission::OrdersWrite)?;   // 403 ApiError::forbidden otherwise
```

Route-permission table (v1):

| Route | Permission |
| --- | --- |
| `GET /api/customers*`, `/api/orders*` (read), `/api/search` | matching `*:read` |
| `POST/PUT/DELETE /api/customers*` | `customers:write` |
| `POST/PUT/DELETE /api/orders*` | `orders:write` |
| order status transition endpoint(s) | `orders:status` |
| `GET /api/invoices*` | `invoices:read` |
| `POST/PUT/DELETE /api/invoices*` | `invoices:write` |
| invoice issue endpoint | `invoices:issue` |
| `GET /api/me` | any authenticated user (incl. pending) |
| `GET /api/ws` | any member with a role (stream carries all entities) |
| `GET/PUT /api/members*` | `members:manage` |
| `GET /api/roles`, `GET /api/permissions` | any member with a role (needed to render the UI) |
| `POST/PUT/DELETE /api/roles*` | `roles:manage` |
| tenant settings endpoints (when added) | `settings:manage` |
| `POST /api/impersonation` | `platform:impersonate` |
| `GET /api/platform/tenants` | `platform:tenants:read` |

Checks are explicit per handler rather than a middleware route-map:
handlers already pull `Extension<AuthContext>`, the codebase is small, and
an explicit `require` line is greppable and testable. An integration test
sweeps every route against every role to pin the table above.

### WS live updates

The live stream broadcasts create/update/delete for all entities, so v1
gates the connection on having any role and does not filter events by
permission. Acceptable while every role can read every entity type (true
of the table above except `invoices:read` for production — noted as a
known coarse edge; per-event filtering by permission is the fix if it
matters).

### Frontend

- `/api/me` gains `role`, `permissions: string[]`, and
  `platform_permissions: string[]`.
- A `usePermissions()` hook over the existing app `AuthContext`; nav
  entries, action buttons, and route guards check it (`can('orders:write')`).
  UI gating is UX only — the API check is the enforcement.
- **Members screen** (`members:manage`): list members, assign/change
  role, see pending members. Invitations remain Clerk's job (its free
  invitation flow); our screen manages roles, not accounts.
- **Roles screen** (`roles:manage`): a role × permission matrix editor —
  rename/create/delete roles, toggle catalog permissions per role. The
  built-in `admin` role renders read-only.
- **Pending screen** for role-less users.
- Platform admins get a minimal **tenant list** view (name, org id,
  created, locale/currency) and the impersonation entry point from the
  impersonation design.

## New endpoints

| Endpoint | Purpose |
| --- | --- |
| `GET /api/members` | list members of the caller's tenant |
| `PUT /api/members/{user_id}` | set role (a member holding the built-in admin role cannot be demoted if they're the last one — 409) |
| `GET /api/roles` | list roles with their permission sets |
| `GET /api/permissions` | the permission catalog (key + description), for the roles editor |
| `POST /api/roles` | create a role |
| `PUT /api/roles/{key}` | rename / edit permission set (400 on built-in) |
| `DELETE /api/roles/{key}` | delete (400 on built-in, 409 if members hold it) |
| `GET /api/platform/tenants` | tenant registry list for platform admins |

Member removal is out of scope for v1: removing someone from the org is
done in Clerk; our member row without a matching Clerk membership is
inert (they can't get a token for that org anymore).

## Interaction with impersonation

- `IMPERSONATOR_USER_IDS` from the impersonation design is replaced by the
  `platform:impersonate` permission.
- An impersonation token's effective identity is the target user, so
  membership resolution naturally yields the **target's** role and
  permissions — support sees exactly what the user sees, including a
  pending screen if that's what the user would get.
- Platform permissions are never granted through an impersonation token
  (`act` claim present → skip platform lookup), so impersonating can only
  narrow privileges.

## Migration / rollout

1. Add `member` table to tenant migrations (startup re-runs migrations
   across all tenant DBs already, so existing tenants pick it up).
2. Add `org_role` to the Clerk session token template (dashboard change,
   free tier).
3. Ship resolution + bootstrap with enforcement behind
   `RBAC_ENFORCE=false` first: permissions are computed and logged
   (`tracing` warn on would-be-denied requests) but not enforced — lets us
   verify the bootstrap assigned sensible roles on the live tenants before
   flipping to `true`.
4. Flip `RBAC_ENFORCE=true`; the flag and the log-only path are removed
   once stable.

Dev mode: `POST /dev/token` accepts an optional `org_role` field
(defaulting to `admin`) so local development and integration tests can
exercise every role.

## Testing

- Unit: default-role seed (exhaustive per role), permission-set
  resolution (unknown catalog values ignored, built-in admin gets the
  full catalog), bootstrap rules (admin claim, member claim, existing row
  never overwritten), last-admin demotion guard, built-in role
  edit/delete rejection.
- Integration (testcontainers): route×role sweep against the enforcement
  table using the default configuration; editing a role's permissions
  changes what its members can do (cache invalidated); pending user gets
  403 + distinguishable code; platform admin without org membership can
  list tenants but cannot read a tenant's orders; role and member data
  isolated per tenant DB.

## Extensions (post-v1)

- Per-event permission filtering on the WS stream.
- Platform console (manage platform admins, tenant lifecycle actions).
- Object-level rules (e.g. sales sees own customers only) — likely as a
  policy layer over repos, not more permission bits.
