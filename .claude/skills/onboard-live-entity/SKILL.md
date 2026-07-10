---
name: onboard-live-entity
description: Wire live-update (WebSocket) support for a new entity end to end — surreal-store live stream, WS hub envelope, frontend cache mapping, and the tests that pin each hop. Use when adding a new entity that must push create/update/delete changes to connected clients, or when asked what live-update onboarding involves.
---

# Onboard a new entity into the live-update pipeline

The pipeline is: SurrealDB `LIVE SELECT` per table → typed stream in
`surreal-store` → per-tenant fan-out in the api WS hub → `change` frames on
`/api/ws` → TanStack Query cache mapping in the frontend. Onboarding a new
entity touches one spot in each hop; the protocol envelope itself never
changes — a new entity only extends the set of `entity` values.

**Prerequisite:** the entity already has its CRUD slice — domain type in
`backend/crates/domain`, a `*_repo.rs` in `backend/crates/surreal-store/src`
with a `TABLE` const and a `Row` struct, and API routes. This skill only
covers the live-update wiring on top of that.

## 1. surreal-store: the Row must be live-stream ready

In the entity's `*_repo.rs` (mirror `customer_repo.rs`):

- `Row` struct and `TABLE` const must be `pub(crate)` — `live.rs` consumes
  both.
- `impl Row { pub(crate) fn key(&self) -> String }` extracting the ULID
  from the `RecordId` — copy the shape from `CustomerRow::key`
  (`customer_repo.rs:109`).
- A `Row → domain` conversion (`From` if infallible, `TryFrom<... Error =
  DomainError>` if parsing can fail — see `OrderRow` vs `CustomerRow`).

## 2. surreal-store: `src/live.rs`

- Add a variant to `LiveChange`: `Foo(ChangeEvent<Foo>)`.
- In `live_changes()`, open the stream the same way as the existing three
  and add it to the `select_all([...])` merge:

  ```rust
  let foos = session
      .select::<Vec<FooRow>>(foo_repo::TABLE)
      .live()
      .await
      .map_err(map_err)?;
  let foos = foos
      .map(|n| map_event(n, FooRow::key, Foo::try_from).map(LiveChange::Foo))
      .boxed();
  ```

- `map_event` already handles the delete case (SurrealDB sends the deleted
  record's content; the protocol wants `data: null`) — nothing per-entity
  to do there.
- **Do not** open live queries anywhere else or restructure how the session
  is held: `LiveChanges` owning the `Arc<Surreal>` session is load-bearing
  (dropped session = silently dead notifications, no error). See
  `docs/adr/0008-live-stream-session-lifetime.md` before touching any of
  that.

## 3. api: `src/ws/hub.rs`

One match arm in `to_server_event`:

```rust
LiveChange::Foo(event) => envelope("foo", event),
```

The wire name is the singular snake_case entity name (`"customer"`,
`"order"`, `"invoice"`). That string is the contract with the frontend —
pick it once, use it identically in step 5.

## 4. Backend tests

- `backend/crates/surreal-store/tests/live.rs` — extend
  `delivers_customer_create_update_delete` (or add a sibling) so the new
  table's create/update/delete each produce the right `LiveChange` variant.
  The existing tests are the template; note they're `#[ignore]`-gated
  (testcontainers).
- `backend/crates/api/tests/ws.rs` — only customer envelopes are asserted
  end-to-end today; add a frame assertion for the new entity if its
  serialization has anything nontrivial (e.g. nested/typed fields in
  `data`). Reuse the `warm_up` probe pattern — asserting on the first
  mutation without it races live-query registration.
- Running them: from `backend/`,
  `cargo test -p surreal-store --test live -- --ignored` and
  `cargo test -p api --test ws -- --ignored`. With podman instead of
  docker, `export DOCKER_HOST="npipe:////./pipe/podman-machine-default"`
  first (root `CLAUDE.md`, "Running tests").

## 5. Frontend: cache mapping

In `frontend/src/lib/ws/applyChange.ts`, add a case for the new `entity`
string mapping to the feature's query keys (each feature exports them from
`frontend/src/features/<entity>/api.ts`, e.g. `customersKeys`). Follow the
existing per-entity handlers:

- create → invalidate everything under the entity's `keys.all`.
- update → `setQueryData` on `keys.detail(id)` from the frame's `data`,
  then invalidate under `keys.all` excluding that detail key (so the
  just-patched payload isn't immediately refetched).
- delete → `removeQueries` on the detail key + invalidate `keys.all`.
- Invalidation is always the acceptable fallback — prefer simple and
  correct over clever cache surgery.

Unknown `entity` values are ignored by design, so a backend deployed ahead
of the frontend is harmless — but that also means a missing case here fails
silently. The Vitest suite next to `applyChange.ts` is what catches it: add
a cache-effect test per action for the new entity, following the existing
per-entity tests there.

If the entity's mutations should feel instant, mirror the optimistic-update
pattern (onMutate snapshot / onError rollback / onSettled invalidate) used
by the existing feature mutation hooks, and add a rollback-on-error test.

## 6. Acceptance

- Backend: fmt + clippy (`-D warnings`) + the two `--ignored` test targets
  above.
- Frontend: the package.json typecheck/lint/test scripts.
- Manual smoke (optional): two browser tabs on the same tenant, mutate in
  one, watch the other update without a refetch spinner.
