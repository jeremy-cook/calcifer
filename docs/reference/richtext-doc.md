# Rich-text document schema

`RichText.doc` (and `PutRichTextRequest.doc`) is a TipTap (ProseMirror) JSON document
held in a string. The server stores it verbatim, but it also parses it on every
`PutRichText`, so the node types and attributes below are part of the API: a client
that writes them differently gets different links, dates and search results.

This file describes what the **server** reads. The authoritative code is
`server/src/links.rs` (`extract_doc_references`, `extract_plain_text`) and
`server/src/embed/chunk.rs` (`chunk_doc`). What the two clients write is described at
the end. See [`data-model.md`](data-model.md) for the RPCs and graph derivation.

---

## The document

- The string is JSON. An empty or blank string is an empty document. Anything else
  that isn't valid JSON makes `PutRichText` fail with `INVALID_ARGUMENT`, and nothing
  is saved.
- The server doesn't validate the document's shape. It doesn't require a root
  `{"type":"doc"}`, and it doesn't check node types against the editor's schema. The
  editor does, so a doc the browser can't render is still accepted.
- The server walks the tree from the root through every `content` array, to any
  depth. A mention inside a list item, table cell, blockquote or `details` block counts
  like one in a top-level paragraph.
- On each node it reads only `type`, `attrs` and `text`, plus `content` to recurse. It
  never reads `marks`.

---

## Nodes the server reads

| Node `type` | Attributes read | Type | Derives |
|---|---|---|---|
| `mention` | `attrs.id` | string: the target entity's id | a link |
| | `attrs.structureType` | string: the target's structure type (`Note`, `Todo`, …) | the link's `target.structure_type` |
| `hashtag` | `attrs.id`, `attrs.structureType` | as for `mention` (a Tag in practice) | a link |
| `dateChip` | `attrs.date` | string: an ISO calendar day, `yyyy-MM-dd` | a `referenced_dates` entry |
| any node | `text` | string | plain text for search and embeddings |

### `mention` and `hashtag`

The server treats the two types the same way. The editor keeps them separate so `#`
chips (Tags) and `@` chips render and autocomplete differently.

- Both `id` and `structureType` must be strings. If either is missing, `null` or not a
  string, the node is skipped: no link, and no error.
- Each distinct `id` in the document gives one link row, scoped to the document's
  property (`LinkRef.source_property_id`, e.g. `content`). If an `id` appears more than
  once, the first occurrence's `structureType` is used.
- An `id` with no entity gives no link row, and isn't an error. The chip stays in the
  document; the editor renders it as a tombstone.
- `structureType` isn't checked against the target's real type. It's recorded on the
  link as written. Relation properties, by contrast, are checked (see `EntityRef`).
- A link keeps its `LinkRef.id` and `created_at` across saves for as long as the
  document still mentions that target.
- A document's links replace that property's previous rich-text links on every save.
  Links from other properties, and relation-property links, aren't touched.

### `dateChip`

- `date` must be a real calendar day in canonical ISO form (`2026-06-13`). A chip whose
  `date` isn't one (`2026-13-45`, `2026-6-13`, `June 13`, `""`) is skipped, as a missing
  or non-string `date` is: it adds nothing to `referenced_dates` and doesn't fail the
  save.
- `Entity.referenced_dates` is the union of the date chips in **all** of the entity's
  rich-text documents, de-duplicated, recomputed on every save of any of them.

### Text

Every string `text` field is collected, depth-first in document order:

- **Full-text search.** `extract_plain_text` joins a document's text fields with single
  spaces and trims the result. An entity's FTS `body` is the plain text of all its
  rich-text documents, joined with spaces.
- **Embeddings.** `chunk_doc` flattens each top-level block (each child of the root's
  `content`) to plain text the same way, drops blank blocks, and packs whole blocks into
  chunks of about 500 words, joined by newlines. A block is never split, so one very
  long block becomes one chunk.

Two consequences:

- Chips have no `text`, so they add nothing to search. A mention's label, a tag's name
  and a date chip's day aren't in the body. Searching for a mentioned name finds the
  mentioned entity by its own name, not the document that mentions it.
- Text nodes are joined with a space even inside one word. `**bold**er` is two text
  nodes, `bold` and `er`, and is indexed as `bold er`.

---

## What the server ignores

- `mention` and `hashtag` attributes other than `id` and `structureType`: `label`,
  `char` and `mentionSuggestionChar`. `label` is a display snapshot taken when the chip
  was inserted; the server never updates it on a rename.
- All marks: `bold`, `italic`, `strike`, `code`, `underline`, `link` (its `href` is not
  a graph link), `highlight`, `textStyle`, `subscript`, `superscript`.
- Every other node type's attributes: heading `level`, code block `language`, image
  `src`, task item `checked`, text alignment and so on. These nodes still contribute
  the `text` of their descendants.
- Nodes with neither `text` nor any of the attributes above (`image`, `hardBreak`,
  `horizontalRule`) contribute nothing.

---

## Example

A document with one paragraph mentioning a Note and a Tag and holding a date chip:

```json
{
  "type": "doc",
  "content": [
    {
      "type": "paragraph",
      "content": [
        { "type": "text", "text": "Plan with " },
        { "type": "mention", "attrs": { "id": "3f2c…", "structureType": "Note", "label": "Roadmap", "char": "@" } },
        { "type": "text", "text": " for " },
        { "type": "dateChip", "attrs": { "date": "2026-06-13" } },
        { "type": "text", "text": " " },
        { "type": "hashtag", "attrs": { "id": "9a1e…", "structureType": "Tag", "label": "planning", "char": "#" } }
      ]
    }
  ]
}
```

On `PutRichText` to the entity's `content`, the server derives:

- `links`: two rows with `source_property_id: "content"`, to `3f2c…` (Note) and `9a1e…`
  (Tag), if both entities exist;
- `referenced_dates`: `["2026-06-13"]`, merged with the dates in the entity's other
  rich-text documents;
- search text: the three text nodes (`"Plan with "`, `" for "`, `" "`) joined with
  spaces and trimmed. The chips add no text.

---

## What the clients write

For reference; the server contract above is what counts.

**Browser editor** (`calcifer/src/editors/tiptap/`). `extensions/entityMention.ts`
defines `mention` (`EntityMention`) and `hashtag` (`HashtagMention`) on top of
`@tiptap/extension-mention`. `components/mention/makeSuggestion.ts` inserts them with
`id`, `label`, `structureType` and `char` (`@` or `#`). TipTap also serializes the
parent extension's `mentionSuggestionChar` (default `@`). The date chip is
`lib/tiptap-extension-date` (`dateChip`, attribute `date`), inserted with an ISO day.
The rest of the schema is StarterKit plus the extensions listed in
`TiptapEditor.tsx` (tables, task lists, details, images, code blocks and so on).
[`../specs/mentions.md`](../specs/mentions.md) covers how chips are inserted.

**MCP server** (`mcp-server/src/markdown/`). `parse.ts` turns agent markdown into a
subset of the same schema:

| Markdown | Node |
|---|---|
| `[[name]]`, `[[name\|label]]` | `mention`, `structureType: "Note"`, `char: "@"`, `id` from `ResolveEntity` by `name` |
| `#name` | `hashtag`, `structureType: "Tag"`, `char: "#"`, `id` from `ResolveEntity` by `name` |
| `yyyy-MM-dd` at a word boundary | `dateChip` |
| headings, `-`/`*` and `1.` lists, `>` quotes, fenced code | `heading` (`level`), `bulletList`/`orderedList`/`listItem`, `blockquote`, `codeBlock` (`language`) |
| `**…**`, `*…*`, `` `…` `` | `bold`, `italic`, `code` marks |

`serialize.ts` goes the other way for `get_note`, writing a mention as `[[label]]`, a
hashtag as `#label` and a date chip as its `date`.
