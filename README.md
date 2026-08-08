# Calcifer

A note-taking app where **entities, links, and tags** bind ideas together — with AI
as a first-class writer, not a bolted-on sidebar.

## Architecture

- `calcifer/` — React + Vite frontend (TipTap editor, TanStack Query + Router)
- `server/` — Rust backend (tonic gRPC + SQLite via sqlx) — the source of truth
- `mcp-server/` — standalone Node MCP server exposing knowledge tools to an AI agent
- `proto/` — protobuf schema, generated to TS (`calcifer/gen/ts`) and to Rust (tonic-build)

The frontend talks to the server over **gRPC-Web** (Vite proxies `/api` → `:8080`).
Links and date references are derived **server-side** from rich-text content inside
`RichTextService.Put` — no client authors links directly, so every writer (browser or
MCP agent) produces an identical, honest graph.

Why the architecture is the way it is: [`docs/adr/`](docs/adr/).
Data model, graph derivation, and the RPC surface:
[`docs/reference/data-model.md`](docs/reference/data-model.md).
What's being built next: [`ROADMAP.md`](ROADMAP.md).

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

Optional — the MCP knowledge tools (backend must be up):

```bash
cd mcp-server && pnpm test:tools   # smoke test
cd mcp-server && pnpm start        # stdio MCP server (registered with Claude Code as `calcifer`)
```

Regenerate protobuf bindings after editing anything under `proto/` — **both** TS
consumers, they currently have separate generated stubs:

```bash
cd calcifer && pnpm proto:gen
cd mcp-server && pnpm proto:gen
```

Rust regen is automatic on `cargo build` (tonic-build).

## Conventions

- **Node version trap:** fresh shells may resolve Node 16 and break pnpm. Prefix
  node/pnpm commands with
  `export PATH="/Users/bebop/.nvm/versions/node/v24.12.0/bin:$PATH"`.
- **Server (Rust):** services in `server/src/services/*.rs`, registered in
  `server/src/main.rs`. Migrations in `server/migrations/` (timestamp-prefixed).
  `sqlx::query!` checks SQL at compile time against the live `server/calcifer.db`
  (apply new migrations before/at build; the binary also runs `migrate!` on boot).
  `server/src/links.rs::extract_doc_references` walks TipTap JSON. Timestamps via
  `ts_from_millis`.
- **Proto:** edit `proto/calcifer/v1/services.proto`, then regen both TS consumers
  (above). Note the `ref` field generates as `r#ref` in Rust.
- **mcp-server (Node/TS):** ops in `src/tools.ts`, MCP wrapper in `src/server.ts`;
  scripts run via `tsx`. Has its OWN proto stubs in `src/gen/` (gitignored) to avoid
  `@bufbuild/protobuf` version skew. Verify with a `tsx` script (see `src/*-test.ts`).
- **FE (React):** hooks in `calcifer/src/model/{api,store,richtext,backlinks}.ts`;
  connect-es **v2** (`createClient`, not `createPromiseClient`). Component style rules
  live in [`CLAUDE.md`](CLAUDE.md).
- **Commits:** one per unit, clear message, ending with the `Co-Authored-By` trailer.

### Known issue: grpcurl hangs

`grpcurl` connects and resolves the method descriptor, then never receives a response:

```bash
grpcurl -plaintext -import-path proto -proto calcifer/v1/services.proto \
  -d '{}' localhost:8080 calcifer.v1.EntityService/List   # hangs
```

The browser gRPC-Web path works fine, so the likely cause is
`tonic_web::GrpcWebLayer` being applied to *all* traffic in `server/src/main.rs:41`
rather than conditionally. Reflection is also not enabled. Until fixed, verify the
backend through the browser or `mcp-server`'s `tsx` test scripts.

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
