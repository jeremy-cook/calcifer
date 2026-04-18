# Calcifer — Phased Application Plan

## Context

`spec.md` specs the rich-text editor internals (mentions / slash menu / drag handle). `editor-comparison-plan.md` tracks editor feature parity. Both are **editor-level**. Most of `editor-comparison-plan.md` Phases 1–6 already ships in `calcifer/src/editors/tiptap/`.

This plan is the **application-level** counterpart: the shell around the editor — layout, sidebar, entity model, Go server, AI. It's the roadmap from "an editor demo" to "a knowledge-capture app with AI as a first-class capability."

### Direction decisions

- **Entity model:** a single polymorphic `Entity` type. A Note is an `Entity` with `structureId = 'Note'` (Capacities-inspired; see `spec.md` §2.4 / §4.1).
- **Schema-first via protobuf.** The communication layer will be RPC, so the data model is authored once in `.proto` and generated into both Go and TS. This starts at Phase 3 — the local store uses proto-generated types from day one, so the Phase 8 Go handoff is a storage swap, not a type rewrite.
- **TS "oneof":** discriminated unions. `@bufbuild/protobuf` generates these from proto `oneof` — so `PropertyValue` is strongly typed end-to-end (no `Record<string, unknown>`).
- **FE first, backend later.** Phases 1–7 are frontend against a localStorage-backed store; Phase 8 swaps to Connect-RPC + Go + SQLite.
- **Single-user.** No auth, no multi-tenant concerns.
- **Dropped:** runtime structure editor (user owns the source), server-authoritative sync, full-text / lexical search. Keeping vector search because it's the RAG substrate — not a user-facing search feature.
- **AI scope:** RAG chat, inline editor AI, auto-extraction on save, agent tool-use.

---

## Data Model (proto, authored in Phase 3)

```proto
syntax = "proto3";
package calcifer.v1;
import "google/protobuf/timestamp.proto";

message EntityRef {
  string id = 1;
  string structure_id = 2;
}

message PropertyValue {
  oneof value {
    string text = 1;
    double number = 2;
    google.protobuf.Timestamp date_value = 3;
    string select = 4;
    EntityRef relation = 5;
    bytes richtext = 6;   // TipTap JSON
  }
}

message Property {
  string id = 1;           // matches PropertyDef.id on the Structure
  PropertyValue value = 2;
}

message LinkRef {
  string id = 1;                // relationship uuid
  EntityRef target = 2;         // { id, structure_id }
  // Always "Dependency" (inline body mention) in v1. If property-level
  // links become a thing, add a `source_property_id` field — the spec.md
  // `'Database'` link type maps to "source_property_id is set".
  string source_property_id = 3; // empty = inline / Dependency link
  google.protobuf.Timestamp created_at = 4;
}

message Entity {
  string id = 1;
  string structure_id = 2;   // 'Note' | 'RootTag' | 'UtilDate' | ...
  string title = 3;
  repeated Property properties = 4;
  repeated LinkRef links = 5;   // outgoing only; backlinks are derived
  google.protobuf.Timestamp created_at = 6;
  google.protobuf.Timestamp updated_at = 7;
}
```

Structures are **source-code constants** (TS + Go), not stored:

```ts
// calcifer/src/model/structures.ts
export const STRUCTURES = {
  Note:    { id: 'Note',    name: 'Note',    properties: [{ id: 'content', type: 'richtext' }] },
  RootTag: { id: 'RootTag', name: 'Tag',     properties: [] },
  UtilDate:{ id: 'UtilDate',name: 'Date',    properties: [] },
} as const
```

Adding a new Structure = editing this file + adding property handling in the rendering code. No DB migration.

---

## Phases

`[X]` done, `[ ]` todo. Phases are independently shippable.

### Phase 0 — Editor Core
Tracked in `editor-comparison-plan.md`. Continues in parallel.

---

### Phase 1 — App Layout Shell

**Goal:** Chrome with collapsible sidebar.

- [ ] `src/layouts/AppShell.tsx` — resizable sidebar (240–360px) + main content
- [ ] `Cmd+\` toggles sidebar; state persists to `localStorage`
- [ ] Top bar: sidebar toggle + breadcrumb placeholder
- [ ] TanStack Router: `/` (home) and `/e/:id` (entity page)
- [ ] Main area mounts the existing `TiptapEditor`

**Deps:** `@tanstack/react-router`, shadcn `resizable`.

---

### Phase 2 — Sidebar Content

**Goal:** Static navigation primitives (no search).

- [ ] **Pinned** section
- [ ] **Structures** — collapsible groups, each listing its entities, with "+ New"
- [ ] **Daily Notes** entry → today's UtilDate entity
- [ ] **Recent** — last-opened entities
- [ ] Item context menu: open, rename, delete, pin

---

### Phase 3 — Proto Schema + Entity Store (FE, localStorage)

**Goal:** Author the proto schema, generate TS types, build the store against those types.

- [ ] Add `buf` toolchain; `proto/calcifer/v1/entities.proto` with the schema above
- [ ] `buf.gen.yaml` → generates TS into `gen/ts/` (also Go into `gen/go/`, unused until Phase 8)
- [ ] Vite path alias `@calcifer/proto` → `gen/ts/`
- [ ] `src/model/structures.ts` — source-of-truth Structure constants
- [ ] `src/model/store.ts` — Zustand store keyed by entity id, values are proto `Entity`
- [ ] `localStorage` persistence adapter (pluggable — swapped in Phase 8)
- [ ] Entity page `/e/:id`: title input + property renderer dispatching on `PropertyValue.case`
  - for `Note`: `richtext` property → mount `TiptapEditor`, serialize doc to `Uint8Array`
- [ ] "+ New Note" creates entity, navigates, focuses title
- [ ] Sidebar reads from store
- [ ] Delete with confirmation

**Why proto now and not at Phase 8:** the `PropertyValue` oneof is the most awkward type in the app. Authoring it in proto first means the TS store has a real discriminated union from day one, and the Phase 8 server work is just storage plumbing — no type rewrite.

---

### Phase 4 — Mentions Wired to the Entity Store

**Goal:** `spec.md`'s mention extensions become real KB operations.

- [ ] Extended mention node attrs: `{ id, label, structureId, char }` (spec.md §4.1)
- [ ] `@` → fuzzy search all entities, with "Create new Note…" tail item
- [ ] `#` → search/create `RootTag` entities
- [ ] `/Structure/` → create path for any registered Structure
- [ ] Inserting a mention appends a `LinkRef` to the current entity's `links[]`
- [ ] Clicking a mention navigates to the target entity

**Reuse:** `src/lib/tiptap-extension-slash-command/` and existing `@tiptap/extension-mention`.

---

### Phase 5 — Backlinks

- [ ] Derived selector `useBacklinks(entityId)` scans all entities' `links[]`
- [ ] Panel at bottom of entity page: grouped by source Structure, with 1-line context snippet
- [ ] Clicking jumps + highlights the mention

---

### Phase 6 — Daily Notes

- [ ] `UtilDate` structure: entities keyed by ISO date
- [ ] Sidebar "Today" routes to today's entity (auto-create on first access)
- [ ] Prev/next day navigation + calendar popover (reuse `src/lib/tiptap-extension-date/`)

---

### Phase 7 — Command Palette (Cmd+K)

- [ ] `Cmd+K` opens palette (use existing `cmdk` dep)
- [ ] Actions: jump to entity, create entity of any Structure, toggle sidebar
- [ ] Fuzzy match over entity titles + structure-scoped create actions

---

### Phase 8 — Go Backend: Connect-RPC + SQLite

**Goal:** Swap the FE storage layer from `localStorage` to a Go server. Types don't change — they're already proto-generated from Phase 3.

**Why Connect-RPC:** protobuf schema → Go server stubs, TS client, *and* the foundation for AI tool-use schemas (Phase 13). Connect speaks HTTP/1.1 JSON to the browser (no envoy/grpc-web proxy) and gRPC server-to-server.

**Repo layout:**
```
calcifer/                   ← Vite app
server/                     ← NEW Go module
  cmd/calcifer/main.go
  internal/
    store/                  ← sqlc-generated
    api/                    ← Connect service impls
    db/migrations/
proto/calcifer/v1/
  entities.proto            ← authored in Phase 3
  services.proto            ← NEW
buf.yaml, buf.gen.yaml
gen/ts/, gen/go/
```

**Services:**
```proto
service EntityService {
  rpc Get(GetRequest) returns (Entity);
  rpc List(ListRequest) returns (ListResponse);
  rpc Create(CreateRequest) returns (Entity);
  rpc Update(UpdateRequest) returns (Entity);
  rpc Delete(DeleteRequest) returns (google.protobuf.Empty);
  rpc Watch(WatchRequest) returns (stream EntityEvent);
}
```

- [ ] Go module + `air` for reload
- [ ] SQLite schema: `entities(id, structure_id, title, created_at, updated_at)` + `properties(entity_id, property_id, value_blob)` + `links(entity_id, link_id, target_id, target_structure_id, source_property_id, created_at)`; `value_blob` is the proto-encoded `PropertyValue`
- [ ] sqlc queries
- [ ] Connect service implementations
- [ ] Replace Zustand's `localStorage` adapter with TanStack Query hooks over the Connect client
- [ ] Vite proxies `/api` → `:8080`
- [ ] `Watch` stream → live sidebar updates

**Deps:** `@connectrpc/connect`, `@connectrpc/connect-web`, `@bufbuild/protobuf`, `@tanstack/react-query`.

---

### Phase 9 — Embeddings (RAG substrate)

**Goal:** Vector index of entity content. Not a user-facing search feature — infrastructure for Phases 10+.

- [ ] `sqlite-vec` extension loaded at startup
- [ ] `embeddings(entity_id, chunk_id, vector, text)` table
- [ ] Chunk TipTap doc by heading + paragraph boundaries (~500 tokens)
- [ ] Background worker: embed queue → LLM provider → upsert
- [ ] Internal `Retrieve(query, k)` RPC (not exposed to user UI)

---

### Phase 10 — AI Chat Panel (RAG)

- [ ] Right panel, `Cmd+.` toggle
- [ ] `AIService.Chat(stream ChatMessage) returns (stream ChatChunk)` — Connect streaming
- [ ] Server: `Retrieve` top-K → inject as system context → stream LLM response
- [ ] Cite source entities as clickable chips in responses
- [ ] Conversations stored as a `Chat` Structure (dogfood the model)

---

### Phase 11 — Inline Editor AI

- [ ] Slash command group `AI`: Summarize, Continue, Rewrite, Ask
- [ ] Selection-aware (or whole-doc if no selection)
- [ ] Streaming insertion into the editor via TipTap transactions
- [ ] Selection bubble: "Ask about this paragraph"

---

### Phase 12 — Auto-Extraction on Save

- [ ] After `Update`, server enqueues extraction job
- [ ] LLM prompt: "List candidate mention targets (people, projects, tags) in this text"
- [ ] Candidates render as dismissable inline badges ("Link to @Alice?")
- [ ] One-click accept rewrites text to a real mention node
- [ ] Never auto-edits — always user-confirmed

---

### Phase 13 — AI Agent with Tool Use

- [ ] Tool schemas auto-generated from Connect service descriptors (proto reflection → LLM tool JSON)
- [ ] Tools: `getEntity`, `createEntity`, `linkEntities`, `listEntitiesByStructure`, `retrieve`
- [ ] `AIService.Agent(stream)` — tool-use loop server-side
- [ ] Chat panel "Agent" mode shows tool calls + results inline
- [ ] `agent_calls` audit table: `(id, tool, args_json, result_json, user_approved, ts)`
- [ ] Destructive tools (delete, bulk-edit) require per-call user confirmation

---

## Feature Matrix

| Feature | Phase | Status |
|---|---|---|
| App layout + collapsible sidebar | 1 | ⬜ |
| Sidebar sections | 2 | ⬜ |
| Proto schema + Entity store | 3 | ⬜ |
| Mentions wired to store | 4 | ⬜ |
| Backlinks | 5 | ⬜ |
| Daily Notes | 6 | ⬜ |
| Command palette | 7 | ⬜ |
| Go server (Connect-RPC + SQLite) | 8 | ⬜ |
| Embeddings (RAG substrate) | 9 | ⬜ |
| AI chat (RAG) | 10 | ⬜ |
| Inline editor AI | 11 | ⬜ |
| Auto-extraction on save | 12 | ⬜ |
| AI agent with tool use | 13 | ⬜ |

---

## Critical Files (Phase 1 only; later phases list their own)

- `calcifer/src/App.tsx` — router root
- `calcifer/src/layouts/AppShell.tsx`
- `calcifer/src/layouts/Sidebar.tsx`
- `calcifer/src/hooks/useSidebarState.ts`

## Existing code to reuse

- `calcifer/src/editors/tiptap/TiptapEditor.tsx` — mounted by the entity page for richtext properties
- `calcifer/src/lib/tiptap-extension-slash-command/` — extend for `/Structure/` create trigger in Phase 4
- `calcifer/src/lib/tiptap-extension-date/` — powers `UtilDate` rendering in Phase 6
- `calcifer/src/components/ui/*` — shadcn primitives (resizable, popover, tabs already present)

---

## Verification

End-to-end through the browser per phase:

- **Phase 1:** `pnpm dev`, `Cmd+\` collapses sidebar, state survives reload.
- **Phase 3:** create a Note, type, reload → content restored. Inspect localStorage: payload is base64-encoded proto.
- **Phase 4:** `@` picks an existing entity; chip renders; `links[]` on source entity contains a `LinkRef`. Clicking navigates.
- **Phase 8:** drop localStorage, create entity via UI → row appears in `sqlite3 calcifer.db`. `Watch` stream: edit in tab A, tab B updates live.
- **Phase 10:** create two thematically related notes; ask "what did I write about X?" — streamed answer cites both.
- **Phase 13:** agent prompt "create a Person called Alice and link her to today's note" → audit log shows `createEntity` + `linkEntities` calls; both artifacts exist in DB.

---

## Deferred / Open Questions

- **LLM provider and integration pattern** — undecided. Decide at the start of Phase 10. Options: Anthropic, OpenAI, local via Ollama, or a provider-agnostic layer.
- **Embedding model** — tied to the LLM decision; decide at Phase 9.
- **Export format** — JSON proto dump vs Markdown-per-entity.
- **User-facing search** — dropped for now; revisit after Phase 10 if chat RAG doesn't cover the discovery use case.
