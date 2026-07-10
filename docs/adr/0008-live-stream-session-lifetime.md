# 0008 — A live-query stream must own its session, or notifications silently stop

## Status

Accepted.

## Context

Every M5 WebSocket integration test failed the same way: the live pipeline
never delivered a single change event, with no error anywhere — the hub's
stream factory succeeded, the three `LIVE SELECT`s opened, and the merged
stream simply never yielded. The store-level test that had validated
`live_changes` passed, so the function "worked" — but only in that test's
exact shape.

Isolation (`crates/surreal-store/tests/live.rs` plus throwaway probe
matrices since removed) produced a confusing pattern before converging:

- Live stream + writes on the same cached `for_tenant()` session →
  delivered.
- Live stream on a `dedicated_for_tenant()` session (any variant tried:
  root clone, fresh connection, fresh connection + full `Store::connect`),
  writes from anywhere → never delivered.
- The live query *was* registered server-side (`INFO FOR TABLE customer`
  showed it) and no error surfaced on either side.

The decisive evidence came from the server's RPC log (`--log trace` on the
SurrealDB container): every `Surreal` handle clone attaches a server-side
session, and **every `Surreal` drop sends a `detach` RPC that destroys its
server-side session** (SDK 3.2 `lib.rs`, `impl Drop for Surreal`). In the
failing cases, a `detach` for the live session appeared right after the
live queries were opened.

Root cause: `live_changes()` returned a stream that did **not** own the
session it was opened on. The SDK's live `Stream` objects don't keep the
session alive either. In the working test the session happened to survive
in the store's moka cache (and in the test body); in the hub's factory the
`Arc<Surreal>` was dropped as soon as the factory returned, the SDK
detached the session, and from then on the server had nowhere to route
notifications for those live queries — they are tagged with and routed via
the registering session. No error is produced anywhere in that path.

Everything else initially suspected — cross-session routing on a
multiplexed connection, connection setup shape, connection ordering, the
`$_table` bind parameter in the fluent API's generated `LIVE SELECT` — was
a red herring explained by which variant happened to keep the session
alive.

## Decision

`live_changes()` returns a `LiveChanges` stream struct that **owns the
`Arc<Surreal>` session** alongside the merged inner streams. The session
now lives exactly as long as the stream, which is also what makes the
teardown contract real (dropping the stream drops the session and KILLs
the live queries server-side).

`Store::dedicated_for_tenant()` stays the cheap first-generation clone of
`root` (ADR 0002-safe) on the shared connection — with the session
lifetime fixed, cross-session delivery on the multiplexed connection works
fine; no dedicated connection per tenant is needed.

## Consequences

- The rule for any future live-query consumer in this codebase: **whatever
  holds the notification stream must also hold the session** — never let
  the stream outlive the last `Arc` to its session. `LiveChanges` enforces
  this by construction for the WS hub.
- `delivers_across_sessions` and `delivers_across_connections` in
  `crates/surreal-store/tests/live.rs` pin the hub's wiring (dedicated
  session watching, cached session writing) and cross-connection delivery
  respectively.
- The silent-failure mode is worth remembering when debugging: a live
  query whose session was detached still shows up in `INFO FOR TABLE` but
  never fires. If live events stop after an SDK upgrade, check session
  lifetimes before anything else.
- Not filed upstream yet; arguably the SDK's live `Stream` should keep its
  session alive (same family of session-lifecycle footguns as ADR 0002).
