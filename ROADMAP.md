# Calcifer — Roadmap

What we're building next. Nothing here is shipped.

For what the app is and how to run it see [`README.md`](README.md); for the data model
and RPC surface see [`docs/reference/data-model.md`](docs/reference/data-model.md);
for why the architecture is the way it is see [`docs/adr/`](docs/adr/).

**Last reviewed:** 2026-10-27, after the Space Race research against `867e468`
(`plans/space-race-research/findings.md`; `SR-` ids below point into its `issues.md`).

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
| 1 | I-80: an open editor saves over an agent's append | M | The research's one lost write, seen once. Data loss, so not deferred; reproduce first. May be the I-49 sighting. |
| 2 | I-81: MCP tools ignore unknown arguments | S | `deny_unknown_fields` on each argument struct. |
| 3 | I-85: date fields can't change month | S | `onMonthChange` into state. |
| 4 | I-88: Details in a table cell hides its text | S | Insert after the table, or refuse in a cell. |
| 5 | I-84: `get_note` drops a mention's label | S | Write `[[Name\|label]]`; show the label in the editor. |
| 6 | I-86: lexical search can't find ISO dates | S | Index date chips by ISO date too. |
| 7 | I-82: a second same-named note is unreachable from MCP | M | Ids in every name-taking tool and in search results. |
| 8 | I-83: non-ASCII names fold three ways | M | One folding rule; the name index needs a migration. |
| 9 | I-87: search gives names (and tags) no weight | M | Ranking change; re-run the research's Q1 scorecard after. |
| 10 | I-49: out-of-order event publishing | M | The client-side guard covers most of it; the server fix waits for a sighting (see I-80). |

---

## After that

### One generic MCP tool surface · size: L

The MCP tools stay Note-shaped and are extended one gap at a time (decided 2026-10-24),
because AI chat clients reach Calcifer through them. When that stops scaling, replace
one tool per structure with a generic surface: an entity addressed by one `ref` string
(`Name`, `Structure/Name`, `DailyNote/<date>` or an id), rich text by `ref` plus
`property`, and about ten tools (`create_entity`, `update_entity`, `find_text`,
`edit_text`, `append_text`, …). Agent tools for creating and completing to-dos belong
here, on the facet.

The Space Race research adds to it: ids accepted everywhere a name is (I-82), unknown
arguments refused (I-81), `link_notes` naming the real reason for a Tag or daily note
(SR-24), the "wikilink(s)" count fixed (SR-06), and which strings became date chips
(SR-15). Agents also need rename, delete and move-a-section: merging one note into another
took about 33 find-and-replace calls (Q2), `append_to_note` can only add at the end, after
`## Sources` (SR-16), and notes that link each other need a stub-then-append (SR-03). Destructive tools (delete, bulk-edit) would want a confirmation
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

### Agent markdown for what the editor holds · size: M

Tables, footnotes and `$` math from the agent silently become text (SR-14, SR-10), and the
editor's tables, Details, underline, colour and alignment reach the agent as placeholders or
plain text; a centred paragraph can't be edited by it at all (SR-27). Either carry each one
or refuse it at write time, and say which. Easier after the converter above.

### Typed relationships · size: L

A relationship carries no type, label or direction, and `get_note`'s `related:` lists only
outgoing ones. In the research all four agents wrote every relationship twice, as a link
and as prose, and couldn't tell "flew on" from "followed by" from "part of" reading the
graph back (SR-23); the Relationships panel interleaves rows of different kinds (SR-30).
Also a bulk link: one hub took 28 `link_notes` calls.

---

## Smaller items

- **Sidebar completion** — recents / pinned / favourites. Structure nav, "+ New" and the
  calendar entry point already ship, as does facet nav ("To-dos"); the sidebar is
  otherwise static.
- **Command palette (`Cmd+K`)** — deferred. `cmdk` is already a dependency. Revisit when
  entity count outgrows the sidebar. At 230 notes in the research, the Notes list sorted
  by last change was the only way to find a note in the browser (SR-18).
- **User-facing search** — FTS5 and hybrid retrieval exist and serve the MCP
  `search_notes` tool, but aren't surfaced in the UI at all. A dedicated search view is
  the open question. The research scored the agent's search on 29 questions: top-1 hits
  10 lexical, 21 semantic, 20 hybrid. Lexical needs every word and has no stemming
  (SR-32); see also I-86 and I-87.
- **One tag-name rule** — tag names are letters and digits with single dots between
  them (`v1.2`); no `_` or `-`. Quick-add follows this, but the agent's markdown
  (`TAG` in `server/src/mcp/markdown/inline.rs`, documented in `docs/reference/richtext-doc.md`)
  still takes `_` and `-`, and nothing validates a Tag name on create (the editor's `#`
  menu, the relation picker, `ResolveEntity`). Enforce it on the server and match it in
  the editor and the docs. Seen in the research: `#apollo-11`, `#apollo_11` and
  `tags: ["Apollo 11"]` all made rule-breaking Tags, the
  picker takes a sentence-ending period into the name (SR-02, SR-07), and a dotted Tag
  can't be found by search (SR-35).
- **Deleting what's linked** — deleting a note, Tag or daily note never says what links
  to it and can't be undone; leftover links become plain text for the agent and dead-end
  grey chips in the browser (SR-36).
- **Telling same-named notes apart** — the `@` picker, the Notes list and search show two
  `Mercury` notes identically (SR-18); pairs with I-82.
- **Date picker jumps** — the `/Date` picker has only month arrows (681 clicks back to
  1969, SR-04), while the calendar's mini calendar has year and month selects.
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
