# Calcifer

A note-taking app where **entities, links, and tags** bind ideas together — with AI
as a first-class writer, not a bolted-on sidebar.

## Architecture

- `calcifer/` — React + Vite frontend (TipTap editor, TanStack Query + Router, Zustand)
- `server/` — Rust backend (tonic gRPC + SQLite via sqlx) — the source of truth
- `server/src/mcp/` — the MCP server (knowledge tools for an AI agent), hosted by the same backend process
- `proto/` — protobuf schema, generated to TS (`calcifer/gen/ts`) and to Rust (tonic-build)
- `scripts/calcifer` — boots the service

The backend is the one service: a single process with two listeners, both on localhost by
default.

| address | path | what |
|---|---|---|
| `CALCIFER_ADDR` (`127.0.0.1:8080`) | `/` | the built UI (`calcifer/dist`, with `index.html` fallback for client-side routes) |
| | `/api/*` | gRPC-Web, which the UI speaks |
| `CALCIFER_MCP_ADDR` (`127.0.0.1:8081`) | `/mcp` | MCP over streamable HTTP, which agents speak; its tools call the services in-process |

In development Vite serves the UI on `:5173` and proxies `/api` to the backend.
Links and date references are derived **server-side** from rich-text content inside
`RichTextService.PutRichText` — no client authors links directly, so every writer (browser or
MCP agent) produces an identical, honest graph.

Why the architecture is the way it is: [`docs/adr/`](docs/adr/).
Data model, graph derivation, and the RPC surface:
[`docs/reference/data-model.md`](docs/reference/data-model.md). The TipTap node types
the server reads from rich-text documents:
[`docs/reference/richtext-doc.md`](docs/reference/richtext-doc.md).
What's being built next: [`ROADMAP.md`](ROADMAP.md).

## Running it

```bash
scripts/calcifer          # build what is stale (UI, server), then serve the UI on http://localhost:8080
scripts/calcifer build    # build only
scripts/calcifer dev      # debug backend + Vite hot reload (UI on http://localhost:5173)
```

The first run creates and migrates `server/calcifer.db`. The build always checks
`sqlx::query!` SQL against the committed cache in `server/.sqlx/` (`server/.cargo/config.toml`
sets `SQLX_OFFLINE`). Settings, all optional:
`CALCIFER_ADDR` (default `127.0.0.1:8080`), `CALCIFER_MCP_ADDR` (default `127.0.0.1:8081`), `CALCIFER_UI_DIR` (default `../calcifer/dist`, relative
to `server/`), `DATABASE_URL` (default `sqlite://./calcifer.db`). Without a UI build the
service still runs, API and MCP only.

Connect Claude Code to the MCP endpoint (the service must be running):

```bash
claude mcp add --transport http calcifer http://localhost:8081/mcp
claude mcp get calcifer          # → Status: ✓ Connected
```

There's no auth (ADR 5), so both ports answer only this machine
(`server/src/security/local.rs`): a request whose `Host` isn't `localhost`, a loopback address
or the listener's own address gets 403, which defeats DNS rebinding. `/mcp` also refuses a
browser request from another site: an `Origin` that isn't loopback gets 403, and a POST
that isn't `application/json` gets 415. Claude Code sends no `Origin`, and browser tools on
`localhost` such as the MCP Inspector pass.

Tools: `search_notes`, `get_note`, `query_text_location`, `edit_note`, `create_note`,
`append_to_note`, `tag_entity`, `link_notes`, `unlink_notes`, `get_backlinks`,
`create_daily_note`, `append_to_daily_note`, `list_structures`. Their tests run with the
rest of the server's: `cd server && cargo test`.

After editing anything under `proto/`, lint it and regenerate the TS bindings:

```bash
cd calcifer && pnpm proto:lint   # buf lint, default rules; must exit 0
cd calcifer && pnpm proto:gen
```

`buf lint` enforces buf's RPC naming: a verb-and-noun RPC name (`GetEntity`), a
`<Rpc>Request` / `<Rpc>Response` pair unique to that RPC, and wrapped responses
(`GetEntityResponse { Entity entity = 1; }`).

Rust regen is automatic on `cargo build` (tonic-build).

## Conventions

- **Node version trap:** fresh shells may resolve Node 16 and break pnpm. The version
  wanted is in `calcifer/.nvmrc` (`nvm use` there); `scripts/calcifer` falls back to the
  newest nvm Node by itself. For other commands, prefix with
  `export PATH="/Users/bebop/.nvm/versions/node/v24.12.0/bin:$PATH"`.
- **Rust toolchain:** `server/rust-toolchain.toml` pins it; rustfmt and the extra clippy
  lints are configured in `server/rustfmt.toml` and `server/Cargo.toml`.
- **Server (Rust):** services in `server/src/services/*.rs`, registered in
  `server/src/main.rs`. Migrations in `server/migrations/` (timestamp-prefixed).
  `sqlx::query!` checks SQL at compile time against the committed cache in `server/.sqlx/`,
  never the live database; the binary runs `migrate!` on boot. After changing a query or
  the schema, refresh the cache against a scratch database and commit `.sqlx/`. Make the
  scratch database by starting the server once with `DATABASE_URL=sqlite://<scratch>` (and
  other ports, `CALCIFER_ADDR` / `CALCIFER_MCP_ADDR`), because the migrations create a
  `vec0` table that `cargo sqlx migrate run` can't load. Then run
  `SQLX_OFFLINE=false DATABASE_URL=sqlite://<scratch> cargo sqlx prepare -- --all-targets`
  (`prepare --check -- --all-targets` verifies the committed cache).
  `server/src/tiptap/` reads TipTap JSON; `server/src/links/` writes the link graph. Timestamps via
  `ts_from_millis`.
- **Proto:** edit `proto/calcifer/v1/services.proto`, then lint and regen both TS consumers
  (above). Note the `ref` field generates as `r#ref` in Rust.
- **MCP (Rust):** tool ops in `server/src/mcp/tools.rs`, the JSON-RPC transport and tool
  schemas in `server/src/mcp/mod.rs`, markdown <-> TipTap in `server/src/mcp/markdown.rs`.
- **FE (React):** `calcifer/src/model/` is the data layer: `api.ts` (client and query
  cache), `sync.ts` and `replica.ts` (the Watch-fed replica), `store.ts` (the hooks and
  mutations components use), and one module per domain (`richtext`, `relationships`,
  `todos`, `calendar`, `dailyNotes`, `dates`, `structures`); connect-es **v2** (`createClient`, not `createPromiseClient`). Component style rules
  live in [`CLAUDE.md`](CLAUDE.md).
- **Commits:** one per unit, clear message, ending with the `Co-Authored-By` trailer.

### No native gRPC

The backend speaks gRPC-Web over HTTP/1.1 under `/api` only, so `grpcurl` can't reach it.
Verify the backend through the browser, an MCP client against `/mcp`, or `cargo test`.

## Editor

`src/editors/` contains `tiptap/` only. An earlier plan compared TipTap against
Lexical side-by-side; Lexical was never implemented and is not a dependency, so the
comparison is cancelled — the Lexical playground remains an informal feature ceiling.

Shipped: rich text and full typographic controls, headings/quote/alignment, all list
types, Shiki-highlighted code blocks, links with autolink + floating editor, tables
(insert, row/col ops, merge/split), images (insert + resize), entity mentions
(`@`, `@<Structure>/`, `#`), the slash menu, drag-handle block reordering, date chips,
and collapsible containers.

Not built: emoji picker, math/KaTeX, table of contents, character count, YouTube and
Excalidraw embeds, multi-column layout, HTML import/export, version history. A general
text-format floating toolbar is also missing (BubbleMenu is used only for link, image,
and table context menus).

How the entity-aware parts actually work:
[`docs/specs/mentions.md`](docs/specs/mentions.md),
[`docs/specs/slash-menu.md`](docs/specs/slash-menu.md),
[`docs/specs/drag-handle.md`](docs/specs/drag-handle.md). The Capacities and BlockNote
prior-art analysis those were built from is in [`docs/research/`](docs/research/).
