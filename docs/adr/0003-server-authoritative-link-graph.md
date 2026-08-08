# 3. Derive the link graph server-side from content

- **Status:** Accepted
- **Date:** 2026-06-12 (landed in `6ac6a8d`, hardened in `5bed065`)

## Context

Mentions (`@`, `#`, `[[wikilinks]]`) appear as chips inside TipTap documents. Those
chips are the thing the author actually sees and edits, but the app also needs a
queryable edge list for backlinks and the calendar references panel.

Originally the client authored both: it inserted the chip *and* wrote a `LinkRef` onto
the entity. That works while the browser is the only writer. Once an MCP agent could
also create notes, it became two independent implementations of "what counts as a
link" — and any disagreement produces a graph that doesn't match what's on the page.

Client-authored links can also drift from content within a single writer: delete a
chip without triggering reconciliation and the edge outlives the text that justified
it.

## Decision

**Content is the only source of truth for the graph.** No client authors links.

`RichTextService.Put` walks the TipTap JSON server-side
(`server/src/links.rs::extract_doc_references`) and re-derives the entity's **full**
outgoing set on every save, inside the write transaction. `Entity.links` is an index
over the document, never authored directly.

Each link records `source_property_id`, scoping it to the property it came from, so an
entity with several richtext or relation properties reconciles each independently.
`referenced_dates` is derived the same way but is entity-scoped — the union of date
chips across all of the entity's documents.

Identity resolution is likewise server-side and single-path:
`EntityService.ResolveByName` get-or-creates by `(structure_type, name)`,
case-insensitively, for both the browser and the agent.

## Consequences

- Every writer produces an identical graph. Adding a third writer is free; it inherits
  correct link semantics by using the same RPC.
- The graph cannot drift from the prose. What you see in the document *is* the edge set.
- Saving one property's document can't clobber another property's links.
- Link reconciliation runs on the write hot path, on the whole document, on every save.
  Fine at current document sizes; a very large document makes every keystroke-debounced
  save do proportional work.
- The FE cannot optimistically render a new backlink without a round trip, because it
  no longer computes edges itself.
- **Deletion becomes deliberately asymmetric.** Deleting an entity leaves mention chips
  pointing at it intact in other documents — they render as clickable tombstones until
  the author removes them, because the document is the author's, not ours to rewrite.
  The server does sweep `links WHERE target_id = ?` so the relational index stays
  referentially clean, which is safe precisely because the server holds the
  authoritative entity set. Dangling references are harmless: backlinks only surface
  live sources.
