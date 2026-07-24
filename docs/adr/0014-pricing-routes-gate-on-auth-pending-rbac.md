# 0014 — Pricing catalog routes gate on authentication, pending RBAC (B1)

## Status

Accepted (temporary — supersede when B1 lands).

## Context

Step 2 of `docs/quote-implementation-plan.md` (catalog admin, A2a) ships the
`/api/pricing/*` CRUD routes. Its task doc (`docs/pricing-admin-plan.md`) pins a
decision that these routes are gated on two new permissions, `pricing:read` and
`pricing:write`, and that the same MR updates the role seed in
`docs/rbac-design.md`.

That decision assumes the RBAC layer exists. It does not. `docs/rbac-design.md`
is roadmap item **B1** and is still "Proposed — not yet implemented". Today
`AuthContext` is `{ user_id, org_id, tenant_db }`: there is no permission
catalog, no role table, no `member` table, and no `auth.require(...)` helper to
call. There is nothing to gate against.

The implementation plan anticipates exactly this: it marks **Steps 1–2 as
"pre-gate-safe"**, notes that only **Step 3** hard-depends on B1, and instructs
that a step shipped ahead of its RBAC dependency record the deviation as an ADR.

## Decision

Ship the pricing catalog routes behind `require_auth` only — authenticated and
tenant-scoped, the same enforcement every other `/api/*` route currently has.
The `pricing:read` / `pricing:write` checks are **not** implemented, and the
role-seed changes in `docs/rbac-design.md` are **not** made in this step.

We do not stub a placeholder `Permission` enum or a no-op `auth.require` now:
building half of B1 to hang one call site on would pre-empt B1's own design
(role storage, bootstrap, cache invalidation, the enforcement flag rollout) and
risk diverging from it. The routes are written so the gate has one obvious home
(the first line of each handler in `crates/api/src/routes/pricing.rs`), and the
route-permission mapping is already tabulated in `docs/rbac-design.md`.

## Consequences

- Any authenticated member of a tenant can read and write that tenant's pricing
  catalog until B1 lands. Tenant isolation (one SurrealDB database per org) is
  unaffected — the existing hard boundary still holds, so this cannot leak one
  tenant's catalog to another; it only fails to distinguish roles *within* a
  tenant.
- This is acceptable for the current stage: the product is pre-B1 across the
  board (customers, orders, and invoices have the same coarse gate today), so
  pricing is not uniquely exposed.

## When B1 lands

1. Add `pricing:read` / `pricing:write` to the permission catalog.
2. Add `auth.require(Permission::PricingRead)` / `PricingWrite` as the first line
   of each handler in `routes/pricing.rs` (reads vs. writes per the mapping in
   `docs/rbac-design.md`).
3. Add the `pricing:*` rows to the default role seed and the route×role sweep
   test.
4. Delete this ADR's "temporary" status.
