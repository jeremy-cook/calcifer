# Calcifer — Remaining Work (parallelizable units)

Decomposition of what's left after M0–M5 so independent contexts can each pick up
a self-contained unit. Read the **Status** + **Conventions** first, then your unit.

---

## Status (what's done)

Branch `data-model-graph-fixes`. Committed & verified:

| commit | milestone |
|---|---|
| `6ac6a8d` | **M0** Rust tonic+SQLite backend; content-derived link graph; Watch streaming |
| `0a91d49` | **M1** browser swapped to the core over gRPC-web (localStorage gone) |
| `8dda6fd` | **M2** `mcp-server/` standalone Node package + smoke test |
| `833e9c9` | **M3** `EntityService.ResolveByName` (get-or-create by name) |
| `75b522d` | **M4** markdown↔TipTap converter (`mcp-server/src/markdown/`) |
| `ba584a7` | **M5** MCP server + 7 knowledge tools (`mcp-server/src/{tools,server}.ts`) |

**Works end-to-end:** an agent (or `mcp-server/src/tools-test.ts`) creates a note with
`[[wikilinks]]`/`#tags`; the server derives links; the browser shows it live.

### Run it
```bash
# backend (auto-creates server/calcifer.db)
cd server && cargo run
# frontend
cd calcifer && pnpm dev            # http://localhost:5173
# mcp tools test (server must be up)
cd mcp-server && pnpm test:tools
```

---

## Conventions (read once)

- **Node version trap:** fresh shells may resolve Node 16 and break pnpm. Always prefix
  node/pnpm commands with `export PATH="/Users/bebop/.nvm/versions/node/v24.12.0/bin:$PATH"`.
- **Server (Rust):** services in `server/src/services/*.rs`, registered in `server/src/main.rs`.
  Migrations in `server/migrations/` (timestamp-prefixed). `sqlx::query!` checks SQL at compile
  time against the live `server/calcifer.db` (apply new migrations before/at build; the binary
  also runs `migrate!` on boot). `server/src/links.rs::extract_doc_references` walks TipTap JSON.
  Timestamps via `ts_from_millis`. Verify with `grpcurl -plaintext -import-path proto -proto
  calcifer/v1/services.proto localhost:8080 calcifer.v1.<Service>/<Method>`.
- **Proto:** edit `proto/calcifer/v1/services.proto`. After changes, regen TS for **both**
  consumers: `cd calcifer && pnpm proto:gen` and `cd mcp-server && pnpm proto:gen`. Rust regen
  is automatic on `cargo build` (tonic-build). Note the `ref` field generates as `r#ref`.
- **mcp-server (Node/TS):** ops in `src/tools.ts`, MCP wrapper in `src/server.ts`; run scripts via
  `tsx`. Has its OWN proto stubs in `src/gen/` (gitignored) to avoid `@bufbuild/protobuf` version
  skew — regen with `pnpm proto:gen`. Verify with a `tsx` script (see `src/*-test.ts`).
- **FE (React):** hooks in `calcifer/src/model/{api,store,richtext,backlinks}.ts`; connect-es **v2**
  (`createClient`, not `createPromiseClient`). Verify in-browser via the preview MCP
  (`.claude/launch.json` already configured; gitignored).
- **Commit per unit** with a clear message; end with the Co-Authored-By trailer.

---

## Units

Each is self-contained. **Dependencies** and **parallel-safe-with** noted. Sizes rough.

### Unit A — M7: Lexical search (FTS5)   ·  size: M  ·  deps: none  ·  parallel with: C, D, E, F
Real full-text search over note names+bodies behind a `SearchService.Search` RPC; re-point the
MCP `search_notes` tool to it (currently a `List`+substring stub in `tools.ts`).

- **Migration** `server/migrations/<ts>_fts.sql`: `CREATE VIRTUAL TABLE entity_fts USING fts5(entity_id UNINDEXED, name, body, tokenize='unicode61');`
- **Plain-text extraction:** add a walker beside `links.rs::extract_doc_references` that collects
  text nodes → a plain string. Keep `entity_fts` in sync (delete+insert by entity_id) inside the
  same transactions as `RichTextService.Put` (body) and `EntityService` create/update (name) /
  delete (remove row).
- **Proto:** `service SearchService { rpc Search(SearchRequest) returns (SearchResponse); }` with
  `SearchRequest{query, uint32 limit}`, `SearchResponse{repeated SearchHit hits}`,
  `SearchHit{Entity entity, string snippet, double score}`. New `server/src/services/search.rs`
  (`MATCH` + `snippet()` + `bm25()`), register in `main.rs`.
- **mcp:** `tools.ts::searchNotes` calls `searchClient.search(...)`; regen mcp stubs.
- **Verify:** seed notes; `grpcurl Search {"query":"..."}` ranked hits; `pnpm test:tools` search still passes.

### Unit B — M8: Semantic retrieval (sqlite-vec + local embedder)   ·  size: L  ·  deps: A (for hybrid)  ·  parallel with: C, D, E, F
Vector embeddings of chunked content, fused with FTS5 into a hybrid `Retrieve`. Default to a
**local** in-process embedding model (no API key, nothing leaves the machine) behind an `Embedder`
trait; pin the vector dimension in the migration.

- Load `sqlite-vec` at startup (`db.rs`); migration: `entity_vec` (`vec0`, fixed dim) + `chunks(entity_id, chunk_index, text)`.
- `server/src/embed/`: `chunk.rs` (split TipTap doc by heading/paragraph ~500 tokens — reuse the
  doc walker), `provider.rs` (`Embedder` trait + `LocalEmbedder`), background `tokio` worker draining
  an embed queue on `Put`/`Create`/`Update` (off the write hot-path).
- **Proto:** extend `SearchService` with `rpc Retrieve(RetrieveRequest) returns (SearchResponse)`
  (`query, k, bool hybrid`); merge FTS5 + vector KNN via reciprocal-rank-fusion.
- **mcp:** `searchNotes` gains a `mode` (lexical|semantic|hybrid), default hybrid.
- **Verify:** two thematically related notes with no shared keywords both rank top for a paraphrased
  query; embeddings land in `entity_vec` after `Put`.

### Unit C — Provenance (Source structure + machine authorship)   ·  size: M  ·  deps: none  ·  parallel with: A, B, D, E, F
Deferred earlier by the user ("doesn't matter who wrote what") — spec kept ready. Adds a `Source`
structure + provenance properties (`authored_by` human|machine, `agent_id`, `sources` relation,
`retrieved_at`) on the existing `PropertyValue` oneof; the MCP write tools stamp them; a
`StructureService.List` RPC makes structures server-authoritative; FE shows a provenance badge.
Skip unless the user re-prioritizes it.

### Unit D — Server RPC gaps + journal tools   ·  size: S  ·  deps: none  ·  parallel with: A, B, C, E, F
Two small RPCs + the MCP tools that need them.
- **`EntityService.ListBacklinks(EntityRef) returns (ListEntitiesResponse)`** — one SQL query
  (`SELECT entity_id FROM links WHERE target_id=?`, then `load_entity`). Replace the client-side
  `backlinkNames` List-scan in `mcp-server/src/tools.ts` with it.
- **`EntityService.CreateDailyNote(string date) returns (Entity)`** — server-side `createDailyNote`
  (date prop + content richtext prop + `name=formatLongDate`; set `date_key`). Then MCP tools
  `create_daily_note(date)` / `append_to_daily_note(date, markdown)` in `tools.ts` + `server.ts`.
- **Also:** populate `entities.date_key` on Create/Update for DailyNotes (deferred from M0; the
  `one_daily_note_per_day` index is inert until then) — map a unique-violation → `Status::already_exists`.
- **Verify:** grpcurl both RPCs; add cases to `tools-test.ts`.

### Unit E — FE polish   ·  size: S  ·  deps: none  ·  parallel with: A, B, C, D, F
- **Title-rename flash:** `calcifer/src/routes/e.$id.tsx` `EntityHeader` — the optimistic
  `useRenameEntity` (`onMutate` awaits `cancelQueries`) can briefly revert the controlled input
  while typing. Fix with local input state seeded from `entity.name` (sync via `useEffect`), firing
  the rename on change (optionally debounced).
- **Share identity with the agent:** re-point `calcifer/src/editors/tiptap/components/mention/
  makeSuggestion.ts` `getOrCreateEntityForMention` (in `store.ts`) to call `entityClient.resolveByName`
  so the browser and MCP agent use one identity path (today the browser get-or-creates client-side).
- **Verify:** preview MCP — type in a title (no flash); `@`-create a tag twice (one Tag).

### Unit F — Workspace consolidation   ·  size: M  ·  deps: none (coordinate merge)  ·  parallel-risky with: A,B (touch mcp gen)
Replace the per-package proto-stub duplication (M2 decision) with a real pnpm workspace: root
`pnpm-workspace.yaml` + `package.json`; hoist deps so `calcifer` and `mcp-server` share ONE
`@bufbuild/protobuf` (removes the type-skew workaround); single generated `gen/` consumed by both.
Stop the FE preview before reinstalling; re-verify FE typecheck + a browser load, and
`mcp-server` typecheck + `pnpm test:tools` after. Lower priority; do when touching build setup.

### Unit G — M5 finalization   ·  size: S  ·  deps: M5 (done)  ·  mostly user action
Register the MCP server with Claude Code and exercise it interactively.
- `mcp-server` runs via `pnpm start` (stdio). Register, e.g. `claude mcp add calcifer -- node --import tsx /Users/bebop/Github/calcifer/mcp-server/src/server.ts` (or a built JS entry).
- **Verify:** from Claude Code, `create_note` / `search_notes` / `get_backlinks`; confirm notes
  appear live in the open browser tab (Watch). Optionally add a `mcp-server/README.md` with setup.

### Unit H — In-app AI chat panel (deferred Phase 10)   ·  size: L  ·  deps: B (retrieval), LLM provider
Right-hand panel that calls the same RPCs (search/retrieve/create) + an `AIService.Chat` streaming
RPC (retrieve context → call Claude → stream → invoke the same write tools). Thin client, zero new
knowledge-ops logic. Only when the user wants it.

---

## Parallelization map

- **Run concurrently now:** A, C, D, E, F (and G as a user action). Each is isolated; the main
  merge contention is the proto file (A & D both add to `services.proto`) and `mcp-server/src/tools.ts`
  (A, D edit it) — coordinate those two, or sequence A→D.
- **B** is best after **A** (hybrid fusion needs FTS5), but its embedding pipeline (chunking, the
  `Embedder` trait, the worker, `entity_vec`) can be built independently and wired to hybrid last.
- **H** is last (needs B + an LLM provider decision).

Recommended first wave for separate contexts: **A (search)**, **D (RPC gaps + journal)**, **E (FE polish)** — fully independent, high value, small/medium.
