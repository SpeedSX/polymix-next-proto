# PolyMix Next — Prototype

See [PLAN.md](PLAN.md) for architecture, decisions, and milestones.

## Run

```
just dev
```

Starts SurrealDB, the API (`AUTH_DEV_MODE=true`, port 8080), and the Vite dev server (port 5173).

## Manually test

- Frontend shell: open http://localhost:5173
- API health: `curl http://localhost:8080/api/health`
- Get a dev JWT (no Clerk needed while `AUTH_DEV_MODE=true`):
  ```
  curl -X POST localhost:8080/dev/token -H 'content-type: application/json' \
    -d '{"user_id":"u1","org_id":"org1"}'
  ```
  Then call any API route with `Authorization: Bearer <token>`.

Run `just check` before committing (fmt, clippy, cargo test, eslint, tsc, vitest).
