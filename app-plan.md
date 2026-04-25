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
- **AI scope:** RAG chat
- **Structure metadata is server-authoritative from Phase 8** — exposed via RPC and treated as runtime configuration on the client (not a compile-time TS constant). Pre-Phase-8 the TS `structures.ts` is the source of truth out of necessity; at Phase 8 it shrinks to a cached fetch. Reasoning: rules like `editable`, `creatable`, property types must be enforced server-side anyway, and a single server-owned copy eliminates client/server drift.
- **Structure metadata is the rendering contract.** The entity page renders properties dynamically from the metadata (input component picked by `type`, read-only vs editable, derived/computed values, select options, relation picker target). Adding a Structure or changing a property type doesn't require an FE deploy. Kept simple in the early phases — full dynamic rendering can land alongside Phase 8.

---

## Data Model (proto, authored in Phase 3)

Authored in `proto/calcifer/v1/entities.proto`; TS generated via `buf` into `calcifer/gen/ts/` and aliased as `@calcifer/proto`.

```proto
syntax = "proto3";
package calcifer.v1;
import "google/protobuf/timestamp.proto";

message EntityRef {
  string id = 1;
  string structure_type = 2;
}

message PropertyValue {
  oneof value {
    string text = 1;
    double number = 2;
    google.protobuf.Timestamp date_value = 3;
    string select = 4;          // option key; allowed set defined on the Structure in source code
    EntityRef relation = 5;
    RichTextRef richtext = 6;   // pointer — actual doc fetched via GetRichText
  }
}

message Property {
  string id = 1;           // matches PropertyDef.id on the Structure
  PropertyValue value = 2;
}

message RichTextRef {
  string entity_id = 1;
  string property_id = 2;
}

message RichText {
  RichTextRef ref = 1;
  string doc = 2;          // TipTap JSON, inspected as text
  google.protobuf.Timestamp updated_at = 3;
}

message LinkRef {
  string id = 1;                // relationship uuid
  EntityRef target = 2;         // { id, structure_type }
  // Always "Dependency" (inline body mention) in v1. If property-level
  // links become a thing, `source_property_id` is set — the spec.md
  // `'Database'` link type maps to "source_property_id is set".
  string source_property_id = 3; // empty = inline / Dependency link
  google.protobuf.Timestamp created_at = 4;
}

message Entity {
  string id = 1;
  string structure_type = 2;  // 'Note' | 'Tag' | 'DailyNote' | ...
  string title = 3;
  repeated Property properties = 4;
  repeated LinkRef links = 5;   // outgoing only; backlinks are derived
  google.protobuf.Timestamp created_at = 6;
  google.protobuf.Timestamp updated_at = 7;
}
```

Structures are **source-code constants** (pre-Phase-8) → **server-owned and exposed via RPC** (Phase 8+). Today's shape in `calcifer/src/model/structures.ts`:

```ts
export const STRUCTURES = {
  Note: {
    type: 'Note', name: 'Note', plural: 'Notes', icon, color,
    properties: [{ id: 'content', type: 'richtext' }],
    // mentionable: true (default)
  },
  Tag: {
    type: 'Tag', name: 'Tag', plural: 'Tags', icon, color,
    properties: [],
    mentionable: false,                    // reach via #, not @
  },
  DateRef: {
    type: 'DateRef', name: 'Date', plural: 'Dates', icon, color,
    properties: [],
    creatable: false,                      // only created via the date input extension
    mentionable: false,                    // reach via date input, not @
    title: { editable: false },            // ISO date set on creation
  },
  DailyNote: {
    type: 'DailyNote', name: 'Daily Note', plural: 'Daily Notes', icon, color,
    properties: [
      { id: 'content', type: 'richtext' },
      { id: 'date',    type: 'relation', editable: false },   // bound to its DateRef
    ],
    creatable: false,                      // opened via sidebar Today / calendar
    mentionable: false,                    // reach via date input / sidebar, not @
    title: {
      editable: false,
      derive: (e) => formatLong(e.properties.date),  // e.g. "Wednesday, April 25, 2026"
    },
  },
} as const
```

Per-property and per-Structure rules carried in this metadata:
- **`mentionable`** (default `true`) — whether `@` autocomplete includes this Structure. Set `false` when the Structure has its own dedicated UI (`#` for Tags, date input for date entities).
- **`creatable`** (default `true`) — whether the "+ New" menu offers it.
- **`editable`** on a property / `title.editable` on a Structure — controls whether the entity page renders an input or read-only display.
- **`title.derive`** — for system-controlled titles (e.g. DailyNote derives its title from its `date` property).

Adding a new Structure = editing this file (later: editing the server-side equivalent) + adding any custom rendering. No DB migration.

---

## Phases

`[X]` done, `[ ]` todo. Phases are independently shippable.

### Phase 0 — Editor Core

Tracked in [`editor-comparison-plan.md`](editor-comparison-plan.md); engineering reference for the mention/slash/drag-handle internals lives in [`plans/editor-internals.md`](plans/editor-internals.md). Continues in parallel.

**Editor capabilities (leadership view):**

The editor is TipTap with rich text, a slash menu for block insertion (headings, lists, code blocks, dividers — *purely block-level; no entity creation*), a drag handle for block reordering aligned to the editor's left gutter, and a date input extension for inserting dates that resolve to `DateRef` entities.

Three trigger characters drive entity workflows inside the editor. **`@`** opens the entity-mention picker (filtered to Structures with `mentionable: true` — today just `Note`); bare `@foo` shows existing matches only. To create a new entity inline, the user narrows explicitly with `@<Structure>/foo` (e.g. `@Note/Alice`, `@Person/Bob`) — when no exact match exists, a "Create new <Structure> 'foo'" tail item appears. **`#`** opens the tag picker; because `#` always implies the `Tag` Structure, its tail item creates new Tags directly. **`/`** is reserved for the slash menu (block insertion only) — entity creation never flows through `/`. The design rule: creation requires unambiguous Structure context (the `/` after `@<Structure>`, or the implicit `Tag` of `#`) so it's always intentional. Inserted mentions register as outgoing `LinkRef`s on the source entity (Phase 4) and are clickable to navigate.

---

### Phase 1 — App Layout Shell ✅

**Detailed plan:** [`plans/phase-1-app-shell.md`](plans/phase-1-app-shell.md)

**Goal:** Chrome with collapsible, resizable sidebar so the app reads as an app, not a single-page editor demo.

- [X] Resizable sidebar (240–360px) + main content
- [X] `Cmd+\` toggles sidebar; collapsed state and width persist across reloads
- [X] Top bar with sidebar toggle
- [X] Routing primitives in place: home, entity page, per-structure list, tag page, calendar
- [X] Main area hosts the editor via the entity page

---

### Phase 2 — Sidebar Content 🟡

**Goal:** Turn the sidebar from navigation chrome into a usable browser and launcher — the primary surface for finding and acting on entities without opening a separate page.

**Shipped**
- [X] **Structure nav** — top-level links for each registered Structure (Notes, Tags, Daily Notes), each opening that structure's list page
- [X] **+ New** — creates an entity in any creatable Structure and jumps to it
- [X] **Calendar entry point** — links to the (Phase 6) calendar route

---

### Phase 3 — Proto Schema + Entity Store (FE, localStorage) ✅

**Goal:** Author the data model in proto, generate TS types, and build a localStorage-backed store against those types so Phase 8 is a storage swap rather than a type rewrite.

- [X] `buf` toolchain configured; `entities.proto` authored per the schema above
- [X] TS types generated and aliased as `@calcifer/proto`; Go generation deferred to Phase 8
- [X] Source-of-truth Structure constants
- [X] Zustand entity store; richtext docs in a parallel slice (so list/metadata reads don't pull in document bodies)
- [X] localStorage persistence using proto JSON serialization
- [X] Entity page renders title + dispatches on each property's oneof case; debounced richtext writes
- [X] Create / rename / delete with confirmation (delete also tears down associated richtext docs)
- [X] Sidebar reads from store

**Naming note:** the proto field is `structure_type` (TS: `structureType`) — earlier drafts of this plan said `structure_id`. Likewise the date Structure is `DailyNote`, not `UtilDate`.

**Why proto now and not at Phase 8:** the `PropertyValue` oneof is the most awkward type in the app. Authoring it in proto first gives the TS store a real discriminated union from day one, and the Phase 8 server work becomes pure storage plumbing.

---

### Phase 4 — Mentions Wired to the Entity Store ✅

**Detailed plan:** [`plans/phase-4-mentions.md`](plans/phase-4-mentions.md)

**Goal:** Turn the mention extensions from cosmetic chips into real knowledge-base operations — every mention is a typed, navigable link backed by entity-store state.

**Shipped**
- [X] Mention nodes carry `{ id, label, structureType, char }` so the chip knows what it points to
- [X] `@` searches `mentionable: true` entities (today: `Note`); `@<Structure>/` narrows to a specific Structure
- [X] `#` searches `Tag` entities
- [X] Create-on-miss tail item appears whenever the Structure is unambiguous: in `@<Structure>/term` (creates an entity of that Structure) and in `#newtag` (creates a Tag). Bare `@term` is reuse-only — creation requires the explicit `/` so it's intentional.
- [X] Inserting a mention reconciles the current entity's outgoing links (add/remove kept in sync with what's actually in the doc)
- [X] Clicking a mention navigates to the target entity

**Explicitly out of scope:**
- Bare `@term` create-on-miss. Creation requires explicit Structure context (`@<Structure>/term`) so users can't accidentally spawn entities from typos.
- `/Structure/Entity/` slash-command create syntax. `/` is reserved for the block-insertion slash menu only — keeping triggers single-purpose.
- Paste/import of Capacities-format mention text. If migration becomes a real need, revisit later.

---

### Phase 5 — Backlinks

- [ ] Derived selector `useBacklinks(entityId)` scans all entities' `links[]`
- [ ] Panel at bottom of entity page: grouped by source Structure, with 1-line context snippet
- [ ] Clicking jumps + highlights the mention

---

### Phase 6 — Dates: `DateRef` + `DailyNote` 🟡

**Goal:** Make dates first-class. Two cooperating Structures: `DateRef` (the date as a referenceable thing) and `DailyNote` (the journal entry for a day).

**Why split:** a date mention in any note (`"meeting on @2024-01-15"`) shouldn't auto-create an empty journal entry — it should create a lightweight `DateRef` whose only purpose is to be linked to and to aggregate backlinks. Opening the journal for a day is a separate, deliberate action that creates a `DailyNote` linked to its `DateRef`. Backlinks on the `DateRef` then naturally aggregate "everything that referenced this date" *and* "the journal for this date" in one place.

- [X] `DailyNote` structure registered
- [ ] `DateRef` structure registered (no body; ISO-date title; `creatable: false`; `mentionable: false`)
- [ ] Date input extension (already in `src/lib/tiptap-extension-date/`) wired to create/resolve `DateRef` entities; inserting a date mention adds a `LinkRef` like any other mention
- [ ] Natural-language date parsing on insert (`"today"`, `"next monday"` → ISO)
- [ ] `DailyNote` carries a `date: DateRef` property (non-editable, set on creation); title derives from that date
- [ ] Sidebar "Today" creates/opens today's `DailyNote` (and its `DateRef` if needed)
- [ ] Calendar popover for prev/next day navigation (`/calendar` route currently a placeholder)

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

  // Richtext — separate from Entity so list responses stay lean
  // and autosave doesn't ship the whole entity on every keystroke.
  rpc GetRichText(RichTextRef) returns (RichText);
  rpc PutRichText(RichText) returns (RichText);
}
```

- [ ] Go module + `air` for reload
- [ ] SQLite schema: `entities(id, structure_id, title, created_at, updated_at)` + `properties(entity_id, property_id, value_blob)` + `richtext(entity_id, property_id, doc, updated_at)` + `links(entity_id, link_id, target_id, target_structure_id, source_property_id, created_at)`. `value_blob` is the proto-encoded `PropertyValue`; when a property's type is richtext, `value_blob` holds the `RichTextRef` pointer and the actual doc lives in the `richtext` table (keyed on `(entity_id, property_id)`)
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
| App layout + collapsible sidebar | 1 | ✅ |
| Sidebar sections | 2 | 🟡 |
| Proto schema + Entity store | 3 | ✅ |
| Mentions wired to store | 4 | ✅ |
| Backlinks | 5 | ⬜ |
| Daily Notes | 6 | 🟡 |
| Command palette | 7 | ⬜ |
| Go server (Connect-RPC + SQLite) | 8 | ⬜ |
| Embeddings (RAG substrate) | 9 | ⬜ |
| AI chat (RAG) | 10 | ⬜ |
| Inline editor AI | 11 | ⬜ |
| Auto-extraction on save | 12 | ⬜ |
| AI agent with tool use | 13 | ⬜ |

---

## Implementation map

Per-phase engineering design, file-level changes, and component shapes live in [`plans/`](plans/). This document stays at the capability/requirements level — see the linked plan in each phase for engineering detail.

Existing code that downstream phases will lean on:
- The TipTap editor (mounted by the entity page for any richtext property)
- The slash-command extension (extended in Phase 4 for `/Structure/` creation)
- The date extension (powers Daily Note rendering in Phase 6)
- shadcn UI primitives (resizable, popover, tabs, dialog already present)

---

## Verification

End-to-end through the browser per phase:

- **Phase 1:** `pnpm dev`, `Cmd+\` collapses sidebar, state survives reload.
- **Phase 3:** create a Note, type, reload → content restored. Inspect localStorage: two keys — one for entity metadata (with a RichTextRef pointer under the `content` property), one for the richtext doc JSON.
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
