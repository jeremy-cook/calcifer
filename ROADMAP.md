# Calcifer — Roadmap

What we're building next. Nothing here is shipped.

For what the app is and how to run it see [`README.md`](README.md); for the data model
and RPC surface see [`docs/reference/data-model.md`](docs/reference/data-model.md);
for why the architecture is the way it is see [`docs/adr/`](docs/adr/).

**Last reviewed:** 2026-10-24, against `connection-status` @ `e3388ab`, where the connection notice is built (stacked on `agent-daily-notes`).

The app currently works end-to-end: Rust/SQLite backend, browser on gRPC-Web, an MCP
agent writing into the same graph, and lexical + semantic retrieval underneath it all.
Everything below is what that foundation is *for*.

---

## Current work

Ranked by value for effort. Effort is a rough guess: S under an hour, M a few hours to a
day, L several days. Defects live in [`ISSUES.md`](ISSUES.md); this is the order to work
them in. Race-condition entries stay deferred (single-user app, see ADR 5).

| # | Item | Effort | Notes |
|---|---|---|---|
| 1 | I-42: a failed rich-text save is dropped silently | M | Medium severity but very unlikely. Retry with backoff, an "unsaved" notice, and a prune guard. Reuses the Watch connection state. |
| 2 | Direct relationships (Next up 1) | L | ADR 12 accepted 2026-10-24. |
| 3 | I-61: names containing `\|` or `]]` | S | Decided 2026-10-24: refuse them on create and rename, no escaping. Direct relationships remove the `link_notes` half. |
| 4 | I-36, I-45, I-73: create-race errors | S each | Deferred unless seen. |
| 5 | I-49: out-of-order event publishing | M | The client-side guard covers most of it; the server fix waits for a sighting. |

## Next up

### 1. Direct relationships · size: L · ADR 12 (accepted 2026-10-24)

Associate two notes without writing a mention into the text. A relationship is a row in a
new `relationships` table, added and removed one at a time by `AddRelationship` and
`RemoveRelationship`, limited by the source structure's `relates_to`, and derived into
`links` under the reserved scope `@related`. The server rejects any property id starting
with `@`, and deleting a target republishes the entities that related to it.

The entity page gets one relationships section: rich-text links, relationships and
back-references, one row per other entity labelled *related* and/or *mentioned*, with an
"Add" picker. The agent's `link_notes` calls `AddRelationship` instead of appending
`Related: [[to]]`, a new `unlink_notes` removes one, and both take a name or an id.
Fixes I-34 for associations (not for `tags`) and removes the `link_notes` half of I-61.
ADR 12's open questions, answered 2026-10-24: referenced dates stay on the calendar, there's
no "Keep" action on mention-only rows yet, and DailyNote keeps `relates_to: []`.

### 2. AI chat panel (RAG) · size: L · **blocked: LLM provider undecided**

The headline feature the retrieval work was built for, and the last piece of "AI as a
first-class writer."

Right-hand panel toggled with `Cmd+.`, backed by a new `AIService.Chat` streaming RPC:
retrieve context via the existing hybrid `Retrieve` → call the model → stream the
response → let it invoke the same write RPCs. Cite source entities as clickable chips.
Conversations stored as a `Chat` Structure, dogfooding the entity model.

Thin client, zero new knowledge-ops logic — every operation it needs already exists as
an RPC. **Decide the provider first** (Anthropic, OpenAI, local via Ollama, or a
provider-agnostic layer); that choice is the only thing blocking this.

### 3. Native gRPC with reflection · size: S · optional

The backend serves only gRPC-Web, under `/api` through axum (see the README's "No native
gRPC"), so `grpcurl` can't reach it and backend checks go through the browser, an MCP
client, or `cargo test`. If a CLI path earns its keep, serve native HTTP/2 gRPC as well,
behind the same local-only checks, and enable tonic reflection so `grpcurl` can list and
call the RPCs without the proto files.

---

## After that

### Inline editor AI · size: M · deps: AI chat panel

Slash-command group `AI`: Summarize, Continue, Rewrite, Ask. Selection-aware, falling
back to whole-doc. Streaming insertion via TipTap transactions. Selection bubble
offering "Ask about this paragraph."

### Auto-extraction on save · size: M · deps: AI chat panel

After `Update`, the server enqueues an extraction job: "list candidate mention targets
(people, projects, tags) in this text." Candidates render as dismissable inline badges
("Link to @Alice?"); one click rewrites the text into a real mention node. **Never
auto-edits** — always user-confirmed.

### In-app agent with tool use · size: L · deps: AI chat panel

Largely proven already by the MCP tools in `server/src/mcp/`; this brings it in-app.
Tool schemas generated from the Connect service descriptors (proto reflection → LLM tool
JSON), an `AIService.Agent(stream)` tool-use loop, and a chat "Agent" mode showing tool
calls and results inline. An `agent_calls` audit table
(`id, tool, args_json, result_json, user_approved, ts`); destructive tools (delete,
bulk-edit) require per-call confirmation. Agent tools for creating and completing
to-dos belong here, on the facet.

Until then the MCP tools stay Note-shaped and are extended one gap at a time (decided
2026-10-24). When this lands, consider a generic surface for both agents: an entity
addressed by one `ref` string (`Name`, `Structure/Name`, `DailyNote/<date>` or an id),
rich text by `ref` plus `property`, and about ten tools (`create_entity`,
`update_entity`, `find_text`, `edit_text`, `append_text`, …) in place of one per
structure. The in-app agent could also be offered over MCP as a single `ask` tool,
beside the explicit tools rather than in place of them.

### Markdown converter from the editor's schema · size: M

`server/src/mcp/markdown.rs` hand-writes the markdown ↔ TipTap mapping and copies
schema facts from `TiptapEditor.tsx` (the link's `target`/`rel` defaults, the mark order,
the attribute allow-list), so a change to the editor's extensions can drift from what
agents read and write. Replace it with a binding to a TypeScript converter built from
the editor's own extension list (`@tiptap/markdown` or `prosemirror-markdown`), run as a
subprocess or in an embedded JS engine. The cost is accepted: a build step, a process
boundary on every tool call, and debugging in two languages.

The TS side must keep what the Rust converter does beyond plain markdown: chips resolved
by name through the Rust services, `[[Structure/Name]]`, date chips, `_` not being
emphasis, `<!-- calcifer:<type> -->` placeholders, and the refusal of an edit over
content markdown can't carry (`inexpressible`). Check first that the library runs
without a DOM and doesn't silently drop marks it can't write. The converter's tests are
the acceptance suite, and I-61's escaping cases belong in it.

---

## Smaller items

- **Sidebar completion** — recents / pinned / favourites. Structure nav, "+ New" and the
  calendar entry point already ship, as does facet nav ("To-dos"); the sidebar is
  otherwise static.
- **Command palette (`Cmd+K`)** — deferred. `cmdk` is already a dependency. Revisit when
  entity count outgrows the sidebar, or when in-app agent tool-use wants a
  keyboard-driven action surface.
- **User-facing search** — FTS5 and hybrid retrieval exist but aren't surfaced in the UI
  at all. Revisit after the chat panel; it may make a dedicated search view redundant.
- **One tag-name rule** — tag names are letters and digits with single dots between
  them (`v1.2`); no `_` or `-`. Quick-add follows this, but the agent's markdown
  (`TAG` in `server/src/mcp/markdown.rs`, documented in `docs/reference/richtext-doc.md`)
  still takes `_` and `-`, and nothing validates a Tag name on create (the editor's `#`
  menu, the relation picker, `ResolveEntity`). Enforce it on the server and match it in
  the editor and the docs.
- **Single-line to-do entry** — quick-add reads `<name> <properties> #tags`, in that
  order, and refuses the whole line if anything is invalid; today it takes the name and
  tags. Add the properties, taking inspiration from Taskwarrior's `task add` (`due:fri`,
  `priority:H`, named dates like `eow`). Decide how a property token is told apart from
  the end of the name, and whether the agent shares the parser.
- **Tags on DailyNote** — ADR 11 gives only Note the `tags` property; adding it to
  DailyNote is a one-line registry edit once there's a reason.
- **Editor gaps** — none blocking: emoji picker, math/KaTeX, table of contents,
  character count, YouTube/Excalidraw embeds, multi-column layout, HTML import/export,
  version history, and a general text-format floating toolbar. Full status in the
  README.
- **Image polish** — caption support and paste-from-clipboard (insert + resize ship).

---

## Deferred by decision

- **Provenance / `Source` structure** — a `Source` Structure plus provenance properties
  (`authored_by` human|machine, `agent_id`, `sources` relation, `retrieved_at`) on the
  existing `PropertyValue` oneof, stamped by the MCP write tools, and a provenance badge
  on the FE. Shelved — "doesn't matter who wrote what." Spec kept here in case
  machine-written notes grow enough to need attribution. (Structures are already
  server-authoritative through `ListStructures`, ADR 7.)
- **Natural-language date parsing** on chip insert (`"next monday"`, `"+3d"`).
  `/today`, `/tomorrow`, `/yesterday` and `/Date` cover the real cases.
- **Runtime structure editor** — the user owns the source; adding a Structure is a code
  edit, not a UI flow. The same goes for facets (ADR 11).
- **Multi-user / sync / auth** — single-user by design.
- **Table of contents** — written on the `tiptap.toc` branch, kept as the tag
  `archive/tiptap.toc` (`TableOfContents.tsx`), never merged to `main`. It and
  `archive/tiptap.excalidraw` come from April, before the history `main` has now, and share
  no commits with it, so bringing them back means porting the files.
- **Excalidraw embed** — a draft kept as the tag `archive/tiptap.excalidraw` (a TipTap
  extension plus modal, view and image components), never merged. Same porting caveat as
  the TOC.

---

## Open questions

- **LLM provider** — blocks the chat panel. Anthropic, OpenAI, local via Ollama, or a
  provider-agnostic layer.
- **Embedding model** — currently local `all-MiniLM-L6-v2` (384-d), pinned in the
  migration. Changing it means a re-embed and a migration.
- **Export format** — JSON proto dump vs Markdown-per-entity. The
  `server/src/mcp/markdown.rs` serializer is most of the latter already.
