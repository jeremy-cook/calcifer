# Calcifer — Roadmap

What we're building next. Nothing here is shipped.

For what the app is and how to run it see [`README.md`](README.md); for the data model
and RPC surface see [`docs/reference/data-model.md`](docs/reference/data-model.md);
for why the architecture is the way it is see [`docs/adr/`](docs/adr/).

**Last reviewed:** 2026-10-26, against `main` @ `56d0ee6`.

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
| 1 | I-49: out-of-order event publishing | M | The client-side guard covers most of it; the server fix waits for a sighting. |

---

## After that

### One generic MCP tool surface · size: L

The MCP tools stay Note-shaped and are extended one gap at a time (decided 2026-10-24),
because AI chat clients reach Calcifer through them. When that stops scaling, replace
one tool per structure with a generic surface: an entity addressed by one `ref` string
(`Name`, `Structure/Name`, `DailyNote/<date>` or an id), rich text by `ref` plus
`property`, and about ten tools (`create_entity`, `update_entity`, `find_text`,
`edit_text`, `append_text`, …). Agent tools for creating and completing to-dos belong
here, on the facet. Destructive tools (delete, bulk-edit) would want a confirmation
step the client can show.

### Markdown converter from the editor's schema · size: M

`server/src/mcp/markdown/` hand-writes the markdown ↔ TipTap mapping and copies
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
  entity count outgrows the sidebar.
- **User-facing search** — FTS5 and hybrid retrieval exist and serve the MCP
  `search_notes` tool, but aren't surfaced in the UI at all. A dedicated search view is
  the open question.
- **One tag-name rule** — tag names are letters and digits with single dots between
  them (`v1.2`); no `_` or `-`. Quick-add follows this, but the agent's markdown
  (`TAG` in `server/src/mcp/markdown/inline.rs`, documented in `docs/reference/richtext-doc.md`)
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

- **In-app AI** — a chat panel (`Cmd+.`, an `AIService.Chat` streaming RPC over the
  hybrid `Retrieve`, answers citing entities as chips, conversations stored as a `Chat`
  Structure), inline editor AI (a slash group: Summarize, Continue, Rewrite, Ask), and
  auto-extraction on save (candidate mentions offered as dismissable badges, never
  auto-edited). Dropped 2026-10-26: existing AI chat clients connect to Calcifer through
  MCP or connectors, so there is no in-app model, provider choice or audit table to
  build. Kept here in case that changes.
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

- **Embedding model** — currently local `all-MiniLM-L6-v2` (384-d), pinned in the
  migration. Changing it means a re-embed and a migration.
- **Export format** — JSON proto dump vs Markdown-per-entity. The
  `server/src/mcp/markdown/write.rs` serializer is most of the latter already.
