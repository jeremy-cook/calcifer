# 1. One polymorphic Entity type, differentiated by Structure

- **Status:** Accepted
- **Date:** 2026-04-01 (landed in `85e9f7f`)

## Context

Calcifer needs to hold several kinds of thing — notes, tags, journal entries, and
later people, projects, and sources. The obvious modelling choice is one type per
kind: a `Note` table, a `Tag` table, and so on.

That choice makes every cross-cutting feature quadratic. Links between arbitrary
kinds need a join table per pair or a polymorphic key anyway. Backlinks need a union
across every table. A mention picker needs to query each kind separately. Adding a
new kind means a migration, new RPCs, new store slices, and new FE routes.

Capacities solves this with "objects" that all share one shape and are differentiated
by a type descriptor. The same pressure applies here, and we are single-user with no
schema-per-tenant concerns to complicate it.

## Decision

There is exactly one entity type. An `Entity` carries `id`, `structure_type`, `name`,
a list of `Property` values, outgoing `links`, and timestamps. A Note is an `Entity`
with `structure_type = 'Note'`; a Tag is one with `structure_type = 'Tag'`.

Behaviour that differs per kind lives in **Structure metadata**, not in the type
system: `mentionable`, `creatable`, `uniqueNames`, per-property `editable`, and
`nameMeta`. The entity page renders properties dynamically by dispatching on each
property's type, so metadata is the rendering contract.

Adding a Structure is an edit to `calcifer/src/model/structures.ts` and
`server/src/structures.rs`, plus any custom rendering. **No DB migration.**

## Consequences

- One route (`/e/$id`), one store slice, one set of CRUD RPCs, one link table, one
  backlinks query — all of it works for every kind of thing automatically.
- Adding a Structure is cheap enough that it isn't a planning event.
- Property values must be a sum type across every kind's needs, which is the single
  most awkward type in the app. See [ADR 2](0002-schema-first-protobuf.md) — this is
  the main reason the model is authored in protobuf.
- Per-kind constraints are enforced softly, in metadata checks, rather than by the
  database. `uniqueNames` on `Tag` is advisory: `ResolveByName` get-or-creates, but a
  bare create or a rename-into-collision can still produce duplicates. Making this
  hard needs a partial unique index and conflict UX.
- Queries that would be trivially typed in a per-kind schema ("all Notes") are a
  filter on `structure_type` and rely on that column being indexed.
