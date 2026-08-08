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

### 2. Workspace consolidation · size: M · deps: none

Housekeeping that unblocks clean shared code, best done next time build setup is open.

Replace the per-package proto-stub duplication with a real pnpm workspace: root
`pnpm-workspace.yaml` + `package.json`, deps hoisted so `calcifer` and `mcp-server`
share ONE `@bufbuild/protobuf` (removing the version-skew workaround), and a single
generated `gen/` consumed by both — so `pnpm proto:gen` stops being a two-command
ritual.

Stop the FE dev server before reinstalling. Re-verify FE typecheck + a browser load,
and `mcp-server` typecheck + `pnpm test:tools`, after.

### 3. Fix the grpcurl hang · size: S

Backend verification currently has no CLI path. `tonic_web::GrpcWebLayer` is applied to
all traffic in `server/src/main.rs:41`, which appears to break plain HTTP/2 gRPC —
grpcurl resolves the method descriptor then hangs. Apply the layer conditionally, and
enable reflection while in there. Details in the README's known-issue section.

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

Largely proven already by `mcp-server/`; this brings it in-app. Tool schemas generated
from the Connect service descriptors (proto reflection → LLM tool JSON), an
`AIService.Agent(stream)` tool-use loop, and a chat "Agent" mode showing tool calls and
results inline. An `agent_calls` audit table
(`id, tool, args_json, result_json, user_approved, ts`); destructive tools (delete,
bulk-edit) require per-call confirmation.

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
  `mcp-server/src/markdown/` serializer is most of the latter already.
