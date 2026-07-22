# 0012 — Postgres store experiment

## Status

Accepted.

## Context

SurrealDB is isolated behind domain repository traits, but choosing a
database based only on implementation appeal would leave performance and
operational ergonomics untested. We need a parity implementation that can
run against PostgreSQL, including hosted Supabase, while keeping the
existing SurrealDB path available as the default during the experiment.

## Decision

Add a switchable PostgreSQL store alongside SurrealDB. One API process uses
one backend, selected by `DB_BACKEND`; an unset value continues to select
SurrealDB.

- PostgreSQL uses `sqlx` and hand-written queries behind the existing domain
  repository traits. Domain types remain database-neutral.
- Tenants use one PostgreSQL schema each, with a shared `system` schema for
  the registry. Every tenant operation sets `search_path` transaction-locally
  through `set_config(..., true)`; session-level tenant state is forbidden.
- API routes and authentication use a backend facade. Concrete store and
  repository types stay inside backend adapters.
- Live changes remain domain types. SurrealDB continues to feed the hub from
  live queries; PostgreSQL mutating handlers publish successful writes to an
  in-process hub through a `ChangePublisher`.
- The PostgreSQL path uses ordinary PostgreSQL features only. Supabase is a
  hosted target, not an application API or authentication dependency.

In-process PostgreSQL publication intentionally supports one API instance.
If the selected backend later needs multiple instances, the upgrade path is
`NOTIFY` in the write transaction plus one `LISTEN` consumer per API
instance feeding its local hub. That is outside this experiment.

## Exit criteria

Both implementations must pass the same HTTP and WebSocket acceptance
suite. We will compare search latency using the same 50k-customer /
200k-order dataset and record developer-experience findings for migrations,
tenant provisioning, query authoring, local operation, and diagnostics.

Those performance numbers and the DX verdict select one backend. The losing
store crate, configuration path, and backend-specific infrastructure are
then deleted; maintaining both indefinitely is not an outcome.

## Consequences

- The prep refactor adds backend and change-publisher seams before any
  PostgreSQL dependency is introduced, with a no-op publisher preserving
  SurrealDB's existing live-query behavior.
- Until a verdict, backend-neutral changes must preserve parity and the
  default SurrealDB behavior.
- Handler publication must include secondary entity changes. Order creation
  can promote a lead customer to active, so the PostgreSQL write boundary
  must surface both the order create and customer update without adding
  database reads to the unchanged SurrealDB request path.
- Schema-per-tenant migrations and PostgreSQL FTS are additional
  implementation work, accepted to produce evidence for the decision.

No deviations from `docs/pg-store-plan.md` are currently required.
