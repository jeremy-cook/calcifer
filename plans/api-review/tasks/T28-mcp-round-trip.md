# T28 · MCP: registry-driven doc property, faithful `get_note`, strict dates, test poll

**Area:** mcp · **Issues:** I-47, I-53, I-58, I-52 (MCP half), I-44

## Background
Read each entry in `ISSUES.md`. Entries written before 2026-09-26 use old RPC names; the
note at the top of "Open" maps them.

- **I-47:** `mcp-server/src/tools.ts` addresses every doc as `propertyId: 'content'`
  (`contentRef`), `backlinkSources` compares link properties with `'content'`, and
  `markdown/verify.ts` writes `'content'`. The browser reads the declared rich-text
  property from the registry (`richTextPropertyIds` in `calcifer/src/model/`).
- **I-53:** `markdown/serialize.ts` writes a `mention` as `[[label]]` and a `hashtag` as
  `#label`, from the stored `label`. Nothing updates it on rename. The browser never
  shows `label` for a live target: `MentionNodeView` renders the target's current name
  (`liveName ?? label`). So the browser has no aliases, and `[[name|label]]` in
  `markdown/parse.ts` only sets that snapshot.
- **I-58 (new):** `parse.ts` reads every `[[…]]` as a Note mention. `get_note` writes a
  mention of a Todo or a DailyNote as `[[Name]]` too, so writing the note back (e.g.
  `create_note`, which replaces) moves that mention to a Note of that name, and creates
  one if it's missing. The live DB has one DailyNote mention in a doc.
- **I-52 (MCP half):** `parse.ts`'s date regex `\d{4}-\d{2}-\d{2}` makes a `dateChip`
  from impossible days such as `2026-13-45`. The server now skips a non-ISO chip (T24).
- **I-44:** `src/tools-test.ts` polls semantic search for 15 s (30 × 500 ms). On a fresh
  server the embedding model is still loading, so the first run can fail.

## Requirements
- **I-47:** get each structure's declared rich-text property from `ListStructures`
  (fetch once per process and reuse it). Address the doc through it for Note and
  DailyNote. If a structure declares several, use the first declared. If it declares
  none, fail with a clear message. Backlink property names are compared against the
  declared rich-text ids, not `'content'`. No `'content'` literal addresses a doc in
  `mcp-server/src`.
- **I-53:** `get_note` writes each chip from its target's **current name**, looked up by
  id (batch or parallel lookups; not one sequential call per chip). The alias form isn't
  kept, since the browser doesn't show aliases. A chip whose target no longer exists is
  written as its label in plain text, so writing it back doesn't create a new entity.
  `parse.ts` keeps accepting `[[name|label]]` as input.
- **I-58:** a mention of a structure other than Note round-trips to the same target:
  - Write it as `[[Structure/Name]]`, using the registry's structure type (e.g.
    `[[Todo/Ship]]`). The browser's mention syntax is `@structure/Name`
    (`docs/specs/mentions.md`).
  - `parse.ts` reads `[[X/Name]]` as a mention of structure X only when X is exactly a
    structure type in the registry; otherwise the whole text is a Note name, as today.
  - The resolver creates a missing target only for a `creatable` structure. For a
    non-creatable one (DailyNote) it only looks up, and a miss becomes plain text.
  - Update the tool descriptions in `server.ts` that explain `[[Wikilinks]]`.
- **Round trip:** writing `get_note`'s output back with `create_note` links the same
  targets and creates nothing. Chips written with `#` stay hashtags.
- **I-52:** the date regex (or a check after it) accepts only real calendar days.
  `2026-02-30` and `2026-13-45` stay plain text.
- **I-44:** make the semantic poll wait long enough for a cold model: up to 60 s, or
  until both notes are found. Keep it failing loudly if they never appear.

## Tests
The MCP server has no unit runner, and you must not run `tools-test.ts` or `verify.ts`
(they need a server). Add cases to them for the tech lead to run:
- `tools-test.ts`: rename a mentioned note and see the new name in `get_note`; round trip
  a note mentioning a Note, a Tag, an alias and a Todo through `get_note` →
  `create_note` and check that the backlinks are unchanged and no entity was created.
- `verify.ts` (or `tools-test.ts`): `2026-02-30` isn't a date chip, `2026-02-28` is;
  `[[Todo/Ship]]` parses to a Todo mention, and `[[Foo/Bar]]` to a Note named `Foo/Bar`.

## Out of scope
Two Notes with the same name: `[[Name]]` resolves to the oldest (D2), so a mention of a
newer namesake round-trips to the oldest. Tag names the `#` syntax can't carry. Server
changes.

## Done when
- `cd mcp-server && pnpm typecheck` passes.
- `grep -rn "'content'" mcp-server/src --include='*.ts'` finds no doc address.
- The tech lead's `pnpm test:tools` passes on the first run against a fresh scratch
  server (report which new assertions to look for).
- `ISSUES.md`: move I-44, I-47, I-53 and I-58 to Resolved. Add a line to I-52 saying that
  the MCP half is done.

## Commits
1. `mcp: address docs by the registry's rich-text property (I-47)`
2. `mcp: write chips from current names and keep their structure (I-53, I-58)`
3. `mcp: parse only real calendar days as date chips (I-52)`
4. `mcp: wait for a cold embedding model in test:tools (I-44)`
5. `docs: resolve I-44, I-47, I-53 and I-58`
