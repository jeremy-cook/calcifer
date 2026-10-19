# 11. To-dos are a facet, not a structure

- **Status:** Accepted
- **Date:** 2026-10-19

## Context

A to-do is its own structure. The registry has a `Todo` entry with `status`, `priority`,
`due`, `tags` and a `content` body, and a to-do is created as a `Todo` from "+ New" or
the to-do list's quick-add. A Note can never become actionable: an entity's structure is
fixed when it's created, and [ADR 10](0010-natural-keys-declared-by-the-registry.md)
relies on that, since `entity_keys` copies `structure_type` on the assumption that it
can't change. Making a note actionable means making a second entity and linking the two.

What makes a to-do a to-do is small. Besides the one registry entry, its behaviour lives
in a handful of frontend files: `model/todos.ts`, the `components/todo/` list, row,
quick-add and status field, the `s.Todo` route, the entity page's overrides and the
calendar's due items. None of it needs a Todo to be a different kind of thing from a
Note. It needs a set of properties, and a way to find the entities that have them.

[ADR 1](0001-polymorphic-entity-model.md) puts every kind of thing in one `Entity`
differentiated by Structure, and [ADR 7](0007-server-owned-structure-registry.md) makes
the server's static registry the one place that data is authored. Neither has a way to
say "this entity is also doing something", independent of what it is.

## Decision

**The `Todo` structure is removed. A to-do is an entity with the `todo` facet: a fixed,
registry-declared group of properties that can be attached to and detached from an
entity whose structure accepts it.** A structure is what an entity *is*; a facet is what
you're doing with it.

Rules:

1. **Facets are fixed.** They are authored in `server/src/structures.rs` next to the
   structures, served by `StructureService.ListStructures`, and changed only with a
   server restart, as ADR 7 does for structures. There are no user-defined facets.
2. **Ids are strings, not a proto enum or oneof.** A new facet is a registry edit with
   no proto change and no migration.
3. **A facet declares properties only.** Keys, naming rules and rich-text bodies belong
   to the structure.
4. **Facet property ids are namespaced and full.** The id is `<facet>.<name>`
   (`todo.status`) everywhere: `PropertyDef.id`, the `properties` table,
   `Entity.properties` and `SetEntityProperty.property_id`. Structure property ids never
   contain a `.`, so the two can't collide.
5. **A structure lists the facets it accepts** (`StructureDef.facets`). Attaching any
   other facet is `FAILED_PRECONDITION`. Only Note accepts `todo`.
6. **Facet values are only written while the facet is attached.** Writing `todo.*` on an
   entity without the facet is `FAILED_PRECONDITION`, so attaching is always explicit.
   Clearing a value is allowed, as for any property.
7. **Detaching deletes the facet's values** and the links derived from them, in the same
   transaction.
8. **Tags are a structure property, not part of the facet.** Note gains
   `relations("tags", "Tags", "Tag")`, moved from Todo; Tag and DailyNote don't get it
   (DailyNote can later). The `tags` property decides what an entity is tagged with. A
   body `#hashtag` only references the Tag: it shows in the Tag's backlinks but doesn't
   tag the entity.

The registry gains a `FacetDef` beside `StructureDef`:

```rust
pub struct FacetDef {
    pub id: &'static str,                    // "todo"
    pub name: &'static str,                  // "To-do"
    pub plural: &'static str,                // "To-dos"
    pub description: &'static str,           // one line, for the agent
    pub properties: &'static [PropertyDef],  // "todo.status", "todo.priority", "todo.due"
}

pub struct StructureDef {
    // ...
    pub facets: &'static [&'static str],     // facet ids this structure accepts
}
```

`todo` declares `todo.status` (open, done; default open), `todo.priority` (none, low,
medium, high; default none) and `todo.due` (a date). Registry tests enforce rules 3 to 5:
every facet property id starts with `<facet id>.`, no facet declares a rich-text
property, no structure property id contains `.`, and every id in `StructureDef.facets`
names a `FacetDef`. Property lookups go through `property_for(structure_type, facets,
property_id)`, which checks the structure first and then the attached facets.

Proto mirrors the registry and adds the membership:

- `FacetDef { id, name, plural, description, properties }` in `structures.proto`, and
  `StructureDef.facets`.
- `ListStructuresResponse.facets`, beside `structures`.
- `Entity.facets`, output only: the attached facet ids in registry order. Their values
  are in `properties` under the namespaced ids.
- `CreateEntityRequest.facets`, attached on create with their select defaults. The
  caller may set values on requested facets only.
- `AttachFacet` and `DetachFacet` on `EntityService`, each taking an `entity_id` and a
  `facet` and returning the `Entity`. Both are `NOT_FOUND` for a missing entity and
  `INVALID_ARGUMENT` for an unknown facet. Attaching an attached facet, or detaching one
  that isn't, returns the entity unchanged with no write and no event. Otherwise the
  call writes (select defaults on attach; the facet's rows and links removed on detach),
  bumps `updated_at` and publishes `upserted`.

Membership is one generic table. Facet values live in `properties` like any other value,
and a link derived from a facet relation property (none today) uses its full id as
`source_property_id`.

```sql
CREATE TABLE entity_facets (
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  facet     TEXT NOT NULL,
  PRIMARY KEY (entity_id, facet)
);
```

The to-do list moves to `/views/todos`, a facet route rather than a structure route. Its
quick-add creates a Note with `facets: ['todo']`, and moves `#word` tokens out of the
name into the `tags` property. For the agent, `list_structures` lists facets after
structures, and `create_note` takes an optional `tags` argument that resolves or creates
each Tag. Hashtags in the body stay references.

## Alternatives considered

- **Keep Todo as a structure.** It works, and costs nothing now, but a note can never
  become a to-do, and a to-do about a note stays a second entity linked to the first.
- **A typed `oneof` per facet in proto** (`TodoFacet todo = …` on `Entity`). Each new
  facet would be a proto change, its values would need a second write path beside
  `SetEntityProperty`, and it goes against ADR 7's choice of data over types for the
  registry.
- **A sidecar table with typed columns** (`todos(entity_id, status, priority, due)`).
  Plain constraints and easy queries, but it brings back the per-kind schema that ADR 1
  decided against, and a new facet needs a migration.
- **Implicit to-do-ness from the presence of a `status` property.** No new concept, but
  there is no membership to index, list or validate against: any structure could grow a
  `status`, and "make this a to-do" has nothing to write until a value is set.
- **User-defined facets.** The value of `todo` is the behaviour the code adds to it (the
  list, the quick-add, the calendar, the status field), which a user-defined facet
  wouldn't have. Worth revisiting together with user-defined structures.
- **A hybrid that keeps a thin Todo structure requiring the facet.** It keeps
  `[[Todo/Name]]` and a "To-do" create option, but there would be two ways to have a
  to-do, and the point of the change is to fold to-dos into Notes.

## Consequences

- An entity is at most one to-do, and a to-do can't span several entities. Several
  to-dos about one Note are separate Notes that mention it. This is accepted.
- Note names absorb to-do names. Notes have no name key, so two Notes, or a Note and a
  former to-do, can share a name, as two Notes already could.
- The `[[Todo/Name]]` link syntax goes away with the structure; a to-do is linked as the
  Note it is.
- Tags move from the to-do to a structure property on Note. A body `#hashtag` no longer
  tags anything: it references the Tag, and the `tags` property is the only way to tag.
- Validation looks up a property across the structure plus the entity's attached
  facets, so every write path needs the entity's facets, not only its structure type.
- Adding a facet, or letting another structure accept one, is a registry edit with no
  proto change and no migration, as adding a structure is under ADR 1.
- Existing `Todo` entities aren't converted. The dev database is recreated after the
  change lands.
