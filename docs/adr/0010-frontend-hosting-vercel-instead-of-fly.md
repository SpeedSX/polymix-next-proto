# 0010 — Frontend hosted on Vercel, not a third Fly app

## Status

Accepted.

## Context

PLAN.md's M6 milestone specifies "Dockerfiles (multi-stage; frontend served
by nginx); fly.toml for api + SurrealDB (volume-backed) + static frontend"
— i.e. three Fly apps, the frontend being an nginx container serving the
Vite build output. That was built (`deploy/Dockerfile.frontend`,
`frontend/fly.toml`, `frontend/nginx.conf`) and validated before this
decision.

For a cost-minimized prototype it's the wrong shape for pure static
content:

- It's a whole VM (even `shared-cpu-1x`/256mb) to serve files a CDN edge
  serves for free. `auto_stop_machines` masks the idle cost but not the
  cold-start latency it adds to the first visitor after a scale-to-zero.
- Cloudflare Pages/Vercel/Netlify are free at this scale, always warm (edge
  CDN, no container to boot), and need zero backend changes: the api's
  `CORS_ALLOWED_ORIGINS` design already takes an arbitrary exact origin, so
  pointing it at a `*.vercel.app` domain instead of a `*.fly.dev` one is a
  config value, not a code change.
- `just dev` already runs Vite's dev server directly and never exercises
  `Dockerfile.frontend`/nginx locally — moving where the *built* bundle is
  hosted doesn't introduce any new dev/prod asymmetry beyond what already
  exists for any Vite app (dev server vs. built static output).

The user already has a Vercel account, so that's the concrete choice here
over Cloudflare Pages/Netlify — the tradeoffs between those three are a
wash at this scale (all free, all edge-CDN, all support build-time env
vars the same way).

Serving the static bundle from the Rust api itself (`tower-http::ServeDir`)
was also considered — cheapest in Fly-resource-count (one fewer app) — but
rejected: it couples the frontend and backend release cycles (a CSS tweak
would force a full Rust rebuild + redeploy) and would only exist in
production, an asymmetry `just dev` doesn't have today.

## Decision

Frontend deploys to Vercel from `frontend/`, build-time `VITE_*` values set
as Vercel environment variables instead of Docker build args.
`deploy/Dockerfile.frontend`, `frontend/fly.toml`, and `frontend/nginx.conf`
are deleted rather than kept as an unused fallback path. `frontend/vercel.json`
holds the one thing Vercel's zero-config Vite detection doesn't cover: the
SPA fallback rewrite (`TanStack Router`'s client-side routes 404 on refresh
without it). See CLAUDE.md's "Deploying: Fly.io (api + SurrealDB) + Vercel
(frontend)" section for the setup and redeploy commands.

`backend/fly.toml` and `deploy/fly.surrealdb.toml` are unaffected — the api
and SurrealDB stay on Fly per the original plan.

## Consequences

- One fewer Fly app, one fewer thing to size/monitor/pay for; the frontend
  gets a CDN and zero cold start for free instead.
- `just build` no longer builds a frontend image (`justfile`'s `build`
  recipe now only builds `polymix-api`) — nothing currently consumes a
  frontend Docker image, so nothing else changes.
- Frontend and backend now redeploy through two different CLIs (`vercel
  deploy --prod` vs. `fly deploy`) instead of one — an acceptable amount of
  extra ceremony for a prototype at this cost/reliability tradeoff.
- If Vercel/the CDN route turns out to not fit later (e.g. a real need for
  server-side rendering, or wanting everything on one platform for
  compliance/ops reasons), reintroducing an nginx Fly app is a small,
  self-contained change — it doesn't touch the api or SurrealDB at all.
