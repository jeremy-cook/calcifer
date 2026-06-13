# Calcifer

A note-taking app where **entities, links, and tags** bind ideas together.

## Architecture

- `calcifer/` — React + Vite frontend (TipTap editor, TanStack Query + Router)
- `server/` — Rust backend (tonic gRPC + SQLite via sqlx) — the source of truth
- `proto/` — protobuf schema, generated to TS (`calcifer/gen/ts`) and to Rust (tonic-build)

The frontend talks to the server over **gRPC-Web** (Vite proxies `/api` → `:8080`).
Links and date references are derived **server-side** from rich-text content inside
`RichTextService.Put` — no client authors links directly, so every writer (browser,
and later the MCP agent) produces an identical, honest graph.

## Dev loop

Two terminals:

```bash
# Terminal 1 — backend (auto-creates server/calcifer.db on first run)
cd server
cargo run            # or: cargo watch -x run

# Terminal 2 — frontend
cd calcifer
pnpm dev             # http://localhost:5173
```

Regenerate protobuf bindings after editing anything under `proto/`:

```bash
cd calcifer && pnpm proto:gen
```

## Roadmap

See [`app-plan.md`](app-plan.md) for the phased plan. Phase 8 (the Rust backend +
browser swap) is complete; upcoming work adds an MCP server so an AI agent can
research topics and add linked, deduplicated notes, plus full-text + semantic search.
