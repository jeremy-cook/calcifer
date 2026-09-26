# T26 · Content links use the target's real type; search text includes chips

**Area:** server, docs · **Issues:** I-51, I-54 · **Decisions:** D12

## Background
`server/src/links.rs` walks a rich-text doc for chips (`walk`) and for plain text
(`extract_plain_text` → `collect_text`). `server/src/embed/chunk.rs` has its own copy of
`collect_text` for embedding chunks. `link_store.rs`'s `replace_scoped_links` writes the
doc's links. `docs/reference/richtext-doc.md` documents all of this, including both
defects. Read I-51 and I-54 in `ISSUES.md`.

- **I-51:** `walk` keeps a `mention`/`hashtag` only if both `attrs.id` and
  `attrs.structureType` are strings, and `replace_scoped_links` records
  `attrs.structureType` as the link's target type unchecked. It already queries whether
  the target exists (and drops a dead one). The editor's default `structureType` is
  `null` (`calcifer/src/editors/tiptap/extensions/entityMention.ts`), so a chip pasted
  from HTML without `data-structure-type` gets no link.
- **I-54:** chips contribute no text to FTS or embeddings, and `collect_text` puts a
  space between every pair of text nodes, even inside one word (`**bold**er` →
  `bold er`).

## Requirements
- **I-51:**
  - A `mention` or `hashtag` needs only a string `attrs.id` to count. `structureType`
    is ignored for links.
  - `replace_scoped_links` reads each target's real `structure_type` (in the query that
    already checks existence) and records that. A dead target is still dropped.
  - Remove `MentionRef.structure_type` if nothing else needs it.
- **I-54:** one rule for both FTS text and embedding chunks, preferably one shared
  function that both `links.rs` and `chunk.rs` call:
  - Inline nodes are concatenated with no separator: `text` (its `text`), `mention` and
    `hashtag` (`attrs.label`, if it's a string), `dateChip` (`attrs.date`, if it's a
    string). A `hardBreak` becomes a space.
  - Blocks (any other node with `content`) are separated by one space for FTS. The
    newline-joined chunks for embeddings stay as they are.
  - Labels are the stored snapshot, which can be stale after a rename. That's accepted.
- **D12:** no reindex of existing documents. Each doc's FTS text and chunks update the
  next time it's saved. Say so in the docs.

## Tests
- `links.rs`: `extract_plain_text` joins `bold` + `er` into `bolder`, includes a
  mention's label, a hashtag's label and a chip's date, and separates paragraphs. Update
  `collects_plain_text_across_blocks` for the new rule.
- `chunk.rs`: the same inline rule inside a chunk.
- `link_store.rs` or `services/richtext.rs`: a `PutRichText` whose mention has a wrong
  `structureType`, and one whose `structureType` is `null`, both produce a link with
  the target's real type. A dead target still gives no link.
- `services/search.rs`: lexical search finds a note by a word split across marks, and by
  the name of an entity it mentions.

## Out of scope
Updating stale labels (I-53 is on the MCP side). Reindexing existing rows (D12). Date
chip format (done in T24). `proto/`.

## Done when
- `cd server && cargo fmt --check` and `cargo test` pass.
- `docs/reference/richtext-doc.md`: the `mention`/`hashtag` rules (only `id` is needed;
  the link records the real type) and the "Text" section (chip text, inline joins, the
  two "consequences" bullets replaced, no reindex) match the code.
- `ISSUES.md`: move I-51 and I-54 to Resolved.

## Commits
1. `server: record content links with the target's real type (I-51)`
2. `server: index chip text and join inline text without spaces (I-54)`
3. `docs: resolve I-51 and I-54`
