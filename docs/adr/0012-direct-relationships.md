# 12. Direct relationships beside content links

- **Status:** Accepted
- **Date:** 2026-10-24. Accepted 2026-10-24.

## Context

[ADR 3](0003-server-authoritative-link-graph.md) makes the server derive every link: from
the mention, hashtag and date chips in a rich-text document, and from `relation` /
`relations` property values. No client writes a link. That keeps the graph true to what
the author sees.

It leaves no way to say "these two belong together" without writing it into the text.
The MCP `link_notes` tool shows the cost: it appends a `Related: [[to]]` paragraph to the
source note. Calling it twice writes the line twice, the target name is formatted by
hand and isn't escaped (I-61), and the link is only as durable as a line of prose the
user may edit away. A mention in a sentence means "this sentence refers to that", and
loses that meaning when it's also the only way to associate two notes.

A `relations` property (`related`, like `tags`) would avoid the prose, but a property
value is a whole list: every add or remove resends it through `SetEntityProperty`, so
two writers adding different targets at once can lose one (I-34). With the agent and the
browser both associating notes, that stops being rare.

The relation property's target is also a single optional structure
(`PropertyDef.target_structure`), so a property can allow one structure or any, never a
chosen set.

"Relation" already names the `relation` / `relations` property kinds. This ADR calls the
new thing a *relationship* throughout, in prose, storage and RPCs, so the two don't blur.

## Decision

**An entity can hold direct relationships: one-way associations to other entities,
stored as rows, added and removed one at a time, and limited by the source's structure
to the structures it may relate to.** The server derives a link from each relationship
as it does from content and properties, and the frontend shows an entity's content links
and relationships in one relationships section.

Rules:

1. **A relationship is a row, not a property value.** It lives in the `relationships`
   table and is written by `AddRelationship` and `RemoveRelationship`, never by
   `SetEntityProperty`. It doesn't appear in `Entity.properties`.
2. **One-way.** A relationship from A to B is A's. B sees it as a back-reference, the
   same way it sees a mention, and doesn't hold a relationship to A. Nothing is written
   on B's side.
3. **The source's structure limits the targets.** `StructureDef.relates_to` lists the
   structures its entities may relate to. A structure with an empty list can't hold
   new relationships. Note declares `relates_to: &["Note"]`.
4. **The limit governs relationships only, never content.** A document can mention
   anything, and every mention is still a link. The limit decides what can be
   deliberately attached, not what can be written.
5. **Writes are idempotent.** Adding an existing relationship, or removing one that
   doesn't exist, returns the entity unchanged with no write and no event, as
   `AttachFacet` does (ADR 11).
6. **No self-relationships.** Relating an entity to itself is `INVALID_ARGUMENT`.
7. **Relationships follow their entities.** Deleting either end deletes the
   relationship. Deleting the target also publishes `upserted` for each source that
   related to it, so every replica drops the row.
8. **The table is the source; its links are derived.** After each relationship write,
   the server runs `replace_scoped_links(entity, "@related", <targets read from the
   table>)` in the same transaction: the same scoped replace that documents and relation
   properties use. Every `links` row is then derived from a stored source (a document, a
   property value or a relationship row), so `links` stays the one index that backlinks,
   the replica and the agent read, and one that could be rebuilt from those sources.
9. **`@` ids are reserved.** Property ids beginning with `@` are reserved for link scopes
   the server derives. Any write that names one (`CreateEntity`, `ResolveEntity`,
   `SetEntityProperty`, `RichTextService.Put`) is `INVALID_ARGUMENT`.

### Registry

```rust
pub struct StructureDef {
    // ...
    pub relates_to: &'static [&'static str],  // structure types; empty = no relationships
}
```

Registry tests check that every entry in `relates_to` names a `StructureDef`, and that no
declared property id starts with `@`. Note declares `&["Note"]`; Tag and DailyNote
declare `&[]`. Widening a list is a registry edit with no migration. Narrowing one leaves
existing rows that break the new rule, which are kept, as ADR 3 keeps chips to deleted
entities, and can always be removed (see Proto).

### Storage

```sql
CREATE TABLE relationships (
  entity_id  TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  target_id  TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (entity_id, target_id)
);
CREATE INDEX relationships_target ON relationships(target_id);
```

The derived `links` rows use the reserved `source_property_id` `@related`, named for the
one kind of relationship there is. A second kind (`@blocks`, say) would sit beside it:
the table gains a `kind` column defaulting to `'related'`, and existing link rows keep
their id. Rule 9 keeps any property, declared or ad hoc, from writing under an `@` scope.

`DeleteEntity` reads `SELECT entity_id FROM relationships WHERE target_id = ?` before it
deletes the entity row, adds those sources to the ones `strip_relation_refs` returns, and
publishes `upserted` for each after commit (rule 7).

### Proto

- `StructureDef.relates_to` (`repeated string`) in `structures.proto`.
- `AddRelationship` and `RemoveRelationship` on `EntityService`, each taking `entity_id`
  and `target_id` and returning the source `Entity`.
- `AddRelationship` checks, in order: the source exists (`NOT_FOUND`); its structure's
  `relates_to` isn't empty (`FAILED_PRECONDITION`); the target exists (`NOT_FOUND`); the
  target isn't the source (`INVALID_ARGUMENT`); the target's structure is in
  `relates_to` (`INVALID_ARGUMENT`). Only then is an existing row a no-op. Validating
  first means the current registry decides whether A may relate to B, not what it
  allowed when a row was written.
- `RemoveRelationship` checks only that the source exists (`NOT_FOUND`). It never
  consults `relates_to`, so a row left behind by a narrowed list, even one narrowed to
  empty, can always be removed. A missing target means there's no row, so the call is a
  no-op.
- A write bumps the source's `updated_at` and publishes `upserted`.
- No new `Entity` field. A relationship arrives as a `LinkRef` in `Entity.links` with
  `source_property_id: "@related"`.

### Frontend

The entity page gets one relationships section built from the entity's rich-text links,
its relationships and the back-references to it. It replaces the Backlinks panel, in its
place on the page. Outgoing links from the entity's own relation properties, such as
`tags`, stay in their own property fields. Referenced dates aren't listed: they belong to
the calendar (ADR 4).

- One row per other entity, labelled with why it's there: *related*, *mentioned*, or
  both. Back-references read *related to this* or *mentions this*. A back-reference that
  comes from a relation property (a Note whose `tags` names this Tag) is a row labelled by
  the property, *via Tags*.
- Only a direct relationship has a remove control. A content-only row jumps to the
  mention instead, since removing it means editing the author's text. With several
  mentions, it jumps to the first in document order, across the entity's documents in
  registry order.
- "Add" opens a picker limited to the structure's `relates_to`.
- Rows with a direct relationship come first, then content-only rows, each group newest
  first by link `created_at`. After 20 rows the rest collapse behind "Show all", so a hub
  note or a busy Tag stays readable.

### Agent

- `link_notes(from, to)` calls `AddRelationship` and writes no text.
- A new `unlink_notes(from, to)` calls `RemoveRelationship`.
- Each argument is a note's name or id. A name is looked up without creating anything
  (`create_if_missing: false`), and a missing name is an error. Notes have no name key
  (ADR 11), so when several share the name the call fails and lists them with their ids,
  and the agent calls again by id.
- `get_note` lists the note's relationships as `related:` in its frontmatter, the
  backlinks output labels `@related` as *related*, and `list_structures` prints each
  structure's `relates_to`.

## Alternatives considered

- **Keep linking through content** (today's `link_notes`). There's nothing new to build,
  but the association is prose: it's duplicated on a second call, it needs escaping,
  and it disappears when the text is edited.
- **A `relations` property on Note** (`related`, like `tags`). It reuses the property
  editor and link derivation, but every change resends the whole list (I-34), and the
  target limit lives on one property rather than on what the structure may relate to.
- **The same property with add and remove operations on `SetEntityProperty`.** It fixes
  I-34, but grows a general-purpose RPC with list-editing semantics only relations use.
- **Link rows only, with no `relationships` table.** One write instead of two, with a
  partial unique index on `links(entity_id, target_id) WHERE source_property_id =
  '@related'`. But `links` would then hold the only copy of something, and stop being an
  index derived from stored sources.
- **Symmetric relationships**, written on both ends. "Related" reads as mutual, but
  every write becomes two rows that must agree, and back-references already show the
  other direction.
- **A `source_kind` column on `links`** instead of the reserved `@related` id. It's
  cleaner to read and rules out a collision by construction, but it's a migration and
  every links query has to consider it. The reserved id fits the existing scoping by
  `source_property_id`, and rule 9 enforces the reservation at runtime.
- **Moving `tags` onto this mechanism.** It would fix I-34 for tags, but tags are labels
  with their own chip row and picker. With one kind of relationship a tag would be
  indistinguishable from a related note, and Note's `relates_to` would have to admit
  Tag, offering tags in the related picker.
- **Limiting content links by `relates_to` too.** It's consistent, but it would mean
  refusing a save because of what the author wrote.

## Consequences

- ADR 3's rule becomes: links come from content, from relation properties, or from
  direct relationships, and the server derives all of them from a stored source.
  Clients still never write a `links` row.
- Two writers adding different relationships can't lose one, so I-34 no longer applies
  to associations. It still applies to `tags`, which stay a property.
- I-61 shrinks to the markdown serializer, since `link_notes` no longer formats link
  syntax.
- The same pair of notes can be linked twice, once by a mention and once by a
  relationship. The relationships section shows them as one row with two labels.
- Removing the mention doesn't remove the relationship, and removing the relationship
  doesn't remove the mention.
- A deleted target leaves no relationship row behind, unlike a chip, which stays in the
  text as a tombstone (ADR 3). Its sources are republished, so no replica shows the row.
- A relationship write re-derives all of the source's `@related` links, work in
  proportion to how many relationships it has.
- A property id starting with `@`, accepted before as an ad-hoc id, is now refused.
- The agent's tool surface changes: `link_notes` changes meaning, `unlink_notes` is
  added, and both take a name or an id.

## Decisions on the open questions (2026-10-24)

- **Referenced dates.** Not listed in the relationships section. They stay on the
  calendar.
- **"Keep" a mention.** No action on content-only rows for now.
- **DailyNote.** `relates_to: &[]` until there's a reason.
