# T02 · ADR 8 (server-owned writes) and ADR 9 (Watch-fed replica)

**Area:** docs · **Issues:** I-20, I-21, I-22, I-23, I-27 (context only; resolves none)

## Goal
Record the two decisions that Phases 2 and 3 implement, before any code depends on them.
Your prompt includes the user's answers to D1–D4. Write the ADRs to match those answers,
not the recommendations below if they differ.

## Background
Read `docs/adr/README.md`, ADR 3 (server-authoritative link graph), ADR 5 (single-user)
and ADR 7 (server-owned registry) for tone and format. Read findings A1–A4 and B3 in
`plans/api-review/review.md`.

## ADR 8 · Clients send intent; the server builds entities
Content to cover:
- **Context:** Create/Update take a whole client-built `Entity`. Defaults and daily-note
  names are built in four places. Create persists client-supplied links (violates ADR 3).
  The MCP server and the frontend use different RPCs for the same jobs.
- **Decision:**
  - `Entity` is output-only.
  - Writes are `Create{structure_type, name?, properties}`, `Rename`, `SetProperty`,
    `Delete` and `Resolve{name | date, create_if_missing}`.
  - The server mints ids, timestamps, default properties and default names.
  - A DailyNote's name is derived from its date on every write.
  - Rich-text documents are addressed by (entity id, declared property id); no
    `RichTextRef` is stored as a property value.
  - The server enforces the registry flags (`creatable`, `name_editable`,
    `unique_names`).
  - Name lookup rule, per D2.
- **Consequences:** a source-breaking proto change, migrated additively (D4); the FE
  builders go away; one code path per job for every client.

## ADR 9 · The frontend is a full replica fed by Watch
- **Context:** ADR 5 (single user) makes a full client copy reasonable, but today it's
  built from List plus invalidations, with a startup race, silent event loss and a
  refetch storm (B3).
- **Decision**, per D1:
  - Watch opens with a snapshot of every entity, then revisioned events.
  - A lagging subscriber gets a fresh snapshot instead of silent drops.
  - The FE applies events to its cache and never refetches the list.
  - Rich-text changes are events too, and Put is conflict-checked.
  - Filtered `List`, `ListBacklinks` and `Search` exist for the agent.
- **Consequences:** the server holds the whole entity set in one response per
  connection; the FE model layer owns a sync module. It would need revisiting if
  Calcifer became multi-user or the data got large.

## Requirements
- Files: `docs/adr/0008-server-builds-entities.md`, `docs/adr/0009-watch-fed-replica.md`
  (use the house slug style), plus rows in `docs/adr/README.md`'s table.
- Status "Accepted". Don't edit existing ADRs; ADRs are immutable. If ADR 8 changes
  something an older ADR says, say so in ADR 8.
- If `docs/adr/README.md`'s "Decisions not yet recorded" mentions the RichTextService
  split, leave it alone.

## Out of scope
`docs/reference/data-model.md` (updated by the tasks that change the proto), code.

## Done when
- Both ADRs exist, match the D1–D4 answers in your prompt, and are linked from the ADR
  index.

## Commit
`docs: add ADR 8 (server builds entities) and ADR 9 (Watch-fed replica)`
