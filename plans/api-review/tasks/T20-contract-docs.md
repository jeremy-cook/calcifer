# T20 · Document the contract and the TipTap doc schema

**Area:** docs, proto comments · **Issues:** I-31 (C2)

## Problem
Parts of the API contract are only implied:
- date values are ISO `yyyy-MM-dd`;
- server-owned fields are output-only;
- relation values need only the target id;
- `RichText.doc` is TipTap JSON that the server parses (`server/src/links.rs`,
  `extract_doc_references` / `extract_plain_text`).

The editor's node types and attributes (mention, hashtag, date chip, …) are therefore
part of the API, but they're undocumented, and the MCP server reimplemented them
(`mcp-server/src/markdown/parse.ts`, `serialize.ts`, `types.ts`).

## Requirements
- **Proto comments:** on each field where it applies, state the format (ISO dates),
  output-only status, and which fields of a relation `EntityRef` the server reads. On
  `RichText.doc`, point to the new schema doc. Comments only, with no field changes.
  Regenerate both TS stubs so the generated doc comments match.
- **`docs/reference/richtext-doc.md` (new):** the document schema the server relies on.
  - List every node and mark type the server reads, with its attributes, types and
    meaning (e.g. mention: target id, structure type, label).
  - Say what the server derives from each (links, `referenced_dates`, plain text for
    search and embeddings).
  - Say what it ignores.
  - Include a minimal JSON example.

  Build this from `links.rs` (authoritative), then cross-check against the FE extensions
  under `calcifer/src/editors/tiptap/extensions/` and the MCP markdown code.
- **Discrepancies:** if the FE or MCP writes attributes the server ignores, or the server
  expects attributes a client doesn't write, **list them in your report**; don't fix
  them.
- Link the new doc from `docs/reference/data-model.md` and from the README's
  architecture section.

## Out of scope
Code changes, and any schema change.

## Done when
- `richtext-doc.md` exists and every node type in `links.rs` is covered.
- The proto comments are updated and the stubs regenerated; everything still builds.
- `docs/specs/` no longer names removed RPCs (e.g. `docs/specs/mentions.md` still says
  `EntityService.ResolveByName`; it's `Resolve` since T07). Leave ADRs as history.
- `ISSUES.md`: move I-31 to Resolved.

## Commits
1. `proto: document field formats and output-only fields (I-31)`
2. `docs: document the rich-text document schema (I-31)`
3. `docs: resolve I-31`
