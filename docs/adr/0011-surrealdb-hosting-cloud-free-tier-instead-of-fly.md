# 0011 — SurrealDB hosted on SurrealDB Cloud's free tier, not a self-hosted Fly app

## Status

Accepted.

## Context

PLAN.md's M6 milestone, and `0010`'s decision that only the api and
SurrealDB remain Fly apps, both assumed a self-hosted SurrealDB on Fly —
built as `deploy/fly.surrealdb.toml` (image-based, volume-backed,
always-on) and validated before this decision.

SurrealDB Cloud (surrealdb.com/cloud) offers a free tier: 1 free instance,
0.25 vCPU / 1GB memory, 1GB storage "free forever," 1GB egress/month.
Checked specifically for compatibility with this codebase before deciding,
since `surreal-store` is version- and auth-model-sensitive
(`docs/surrealdb-rust-sdk-notes.md`, ADR 0002, ADR 0006):

- **Root auth works identically to self-hosted.** Cloud docs show the same
  `db.signin(Root { username, password })` after `use_ns`/`use_db` that
  `Store::try_connect` already does — no code path exists that's specific
  to self-hosted auth.
- **Connection is the same SDK call.** `any::connect("wss://<endpoint>")`
  is exactly `surrealdb::engine::any::connect` as already used in
  `store.rs` — only the URL value changes, from
  `ws://polymix-db.internal:8000` to the Cloud instance's `wss://` endpoint.
- **Version is selectable** at instance creation ("select the latest
  version, recommended") — pick a 3.x release to stay on the SDK 3
  independent-session-clone multi-tenancy model this codebase depends on
  (PLAN.md's "SurrealDB connections & tenant sessions" section); this
  needs a one-time check at instance-creation time, not an ongoing risk.

The real constraint is the 1GB storage cap: past it, the free tier goes
**read-only** (no error budget for silently blocking writes mid-demo, so
staying well under it matters). The small `ua` demo tenant (100
customers/1,000 orders, `just seed-ua`) fits comfortably. The perf-seed
tenant (50k customers/200k orders, `just seed`) used for the M6 k6/NFR
pass does not.

## Decision

SurrealDB runs on a SurrealDB Cloud free-tier instance for now.
`backend/fly.toml`'s `SURREALDB_URL` points at the instance's `wss://`
endpoint; `SURREALDB_USER`/`SURREALDB_PASS` Fly secrets hold its root
credentials. Real M6 perf testing (k6, NFR targets against the full
50k/200k seeded tenant) is deferred — this decision only covers "get the
demo running," not the perf pass.

Unlike the frontend/Vercel swap in `0010`, `deploy/fly.surrealdb.toml` is
**kept, not deleted** — its header comment now says so explicitly. It's
the documented fallback for when the free tier's 1GB cap becomes limiting
(the perf pass, or real usage growth): still valid, still gives a bigger
volume and private 6PN networking to the api, nothing in it needs to
change to become the primary path again.

## Consequences

- Zero DB hosting cost and zero DB ops (no volume to provision, no image
  version to keep in lockstep with compose) for the initial "see it
  running" goal, at no code cost — this is a config change end to end.
- SurrealDB traffic now crosses the public internet (`wss://`, encrypted,
  but not Fly's private 6PN) instead of an internal address. Latency and
  reliability implications untested; acceptable for "just want to see it
  running," worth watching if it demos poorly.
- 1GB egress/month is also a limit worth remembering if the demo tenant
  sees real traffic, though the `ua` tenant's scale makes this unlikely to
  bite first.
- The M6 "done when" perf numbers in `docs/perf.md` cannot be produced
  against this hosting choice at the originally seeded scale — re-run them
  against `deploy/fly.surrealdb.toml` (or a paid Cloud tier) when that work
  actually happens.
