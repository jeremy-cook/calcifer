# Calcifer — Roadmap

What we're building next. Nothing here is shipped.

For what the app is and how to run it see [`README.md`](README.md); for the data model
and RPC surface see [`docs/reference/data-model.md`](docs/reference/data-model.md);
for why the architecture is the way it is see [`docs/adr/`](docs/adr/).

**Last reviewed:** 2026-08-08, against branch `data-model-graph-fixes` @ `4bfd919`.

The app currently works end-to-end: Rust/SQLite backend, browser on gRPC-Web, an MCP
agent writing into the same graph, and lexical + semantic retrieval underneath it all.
Everything below is what that foundation is *for*.

---

## Next up

### 1. AI chat panel (RAG) · size: L · **blocked: LLM provider undecided**

The headline feature the retrieval work was built for, and the last piece of "AI as a
first-class writer."

Right-hand panel toggled with `Cmd+.`, backed by a new `AIService.Chat` streaming RPC:
retrieve context via the existing hybrid `Retrieve` → call the model → stream the
response → let it invoke the same write RPCs. Cite source entities as clickable chips.
Conversations stored as a `Chat` Structure, dogfooding the entity model.

Thin client, zero new knowledge-ops logic — every operation it needs already exists as
an RPC. **Decide the provider first** (Anthropic, OpenAI, local via Ollama, or a
provider-agnostic layer); that choice is the only thing blocking this.

### 2. Workspace consolidation · done

Obsolete: the MCP server moved into the Rust backend (`server/src/mcp/`), so `calcifer/`
is the only TS package and there are no duplicate proto stubs left to consolidate.

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

Largely proven already by the MCP tools in `server/src/mcp/`; this brings it in-app. Tool schemas generated
from the Connect service descriptors (proto reflection → LLM tool JSON), an
`AIService.Agent(stream)` tool-use loop, and a chat "Agent" mode showing tool calls and
results inline. An `agent_calls` audit table
(`id, tool, args_json, result_json, user_approved, ts`); destructive tools (delete,
bulk-edit) require per-call confirmation.

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
the acceptance suite.

---

## Smaller items

- **Sidebar completion** — recents / pinned / favourites. Structure nav, "+ New" and the
  calendar entry point already ship; the sidebar is otherwise static.
- **Command palette (`Cmd+K`)** — deferred. `cmdk` is already a dependency. Revisit when
  entity count outgrows the sidebar, or when in-app agent tool-use wants a
  keyboard-driven action surface.
- **User-facing search** — FTS5 and hybrid retrieval exist but aren't surfaced in the UI
  at all. Revisit after the chat panel; it may make a dedicated search view redundant.
- **Editor gaps** — none blocking: emoji picker, math/KaTeX, table of contents,
  character count, YouTube/Excalidraw embeds, multi-column layout, HTML import/export,
  version history, and a general text-format floating toolbar. Full status in the
  README.
- **Image polish** — caption support and paste-from-clipboard (insert + resize ship).
- **Connection status** — pages that read the Watch replica render nothing until the
  first snapshot, and Watch retries silently, so with the server unreachable a deep
  link to `/e/<id>` stays blank forever. Have the replica track the stream's state
  (connecting, live, retrying) in `model/replica.ts`, set it from `model/sync.ts`, and
  show "Can't reach the server" once the first connection attempt fails. Covers every
  page, not just `/e/<id>`. Chosen over a `GetEntity` on load, which fails the same way
  and renders a page without its relations.

---

## Deferred by decision

- **Provenance / `Source` structure** — a `Source` Structure plus provenance properties
  (`authored_by` human|machine, `agent_id`, `sources` relation, `retrieved_at`) on the
  existing `PropertyValue` oneof, stamped by the MCP write tools, with a
  `StructureService.List` RPC making structures server-authoritative and a provenance
  badge on the FE. Shelved — "doesn't matter who wrote what." Spec kept here in case
  machine-written notes grow enough to need attribution.
- **Natural-language date parsing** on chip insert (`"next monday"`, `"+3d"`).
  `/today`, `/tomorrow`, `/yesterday` and `/Date` cover the real cases.
- **Runtime structure editor** — the user owns the source; adding a Structure is a code
  edit, not a UI flow.
- **Multi-user / sync / auth** — single-user by design.

---

## Open questions

- **LLM provider** — blocks the chat panel. Anthropic, OpenAI, local via Ollama, or a
  provider-agnostic layer.
- **Embedding model** — currently local `all-MiniLM-L6-v2` (384-d), pinned in the
  migration. Changing it means a re-embed and a migration.
- **Export format** — JSON proto dump vs Markdown-per-entity. The
  `server/src/mcp/markdown.rs` serializer is most of the latter already.
