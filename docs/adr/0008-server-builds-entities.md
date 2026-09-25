# 8. Clients send intent; the server builds entities

- **Status:** Accepted
- **Date:** 2026-09-25 (context for I-20, I-21, I-22 and I-23; implemented by later tasks)

## Context

[ADR 3](0003-server-authoritative-link-graph.md) and
[ADR 7](0007-server-owned-structure-registry.md) put the server in charge of the link
graph and the structure registry. The write path didn't follow:

- `Create` and `Update` take a whole client-built `Entity`, including fields only the
  server should own: `id`, `links`, `referenced_dates`, `created_at`, `updated_at`.
  `Create` persists the client-supplied `links` and `referenced_dates`, which breaks
  ADR 3's rule that no client authors links. `Update` silently ignores the same fields.
- A new entity's defaults (rich-text refs, select defaults, the default "Untitled X"
  name, a DailyNote's date and name) are built in four places: two frontend builders in
  `model/store.ts` and two server builders in `services/entity.rs`, one of which says it
  "mirrors the FE's buildEntityMessage".
- A rich-text property stores a `RichTextRef` value that is just (entity id, property
  id), both known from context. It has to be created on every new entity, the server
  never checks it points at its owner, and a missing value makes the editor render
  nothing.
- The MCP server and the frontend do the same jobs through different RPCs. The agent
  creates daily notes with `CreateDailyNote` and recovers from `AlreadyExists` by listing
  every DailyNote; the frontend uses `Create` and names the note on the client. Moving a
  daily note with `SetProperty(date)` leaves its name stale, and nothing enforces
  `name_editable = false`.
- Name lookup (`[[wikilinks]]`, `@` mentions, the agent's `get_note`) runs against Notes
  and Todos, whose names aren't unique, with `LIMIT 1` and no `ORDER BY`. Which of two
  same-named notes you get is undefined, and an unknown `structure_type` still creates
  an entity.

## Decision

**Clients send intent. The server builds, validates and names every entity.**

- **`Entity` is output-only.** No request message contains one.
- **Writes are intent-shaped RPCs:**
  - `Create { structure_type, name?, properties }`
  - `Rename { id, name }` (replaces `Update`)
  - `SetProperty`
  - `Delete`
  - `Resolve { oneof key { name, date }, create_if_missing } → { entity, created }`,
    one get-or-create for both clients. It replaces `ResolveByName` and
    `CreateDailyNote`.
- **The server mints** ids, timestamps, default property values and default names. The
  frontend's entity builders are deleted, not kept as a fallback.
- **A DailyNote's name is derived from its date on every write**, by the server. Moving a
  daily note is a plain `SetProperty(date)`; there is no client-side rename.
- **Rich-text documents are addressed by (entity id, declared property id).** The
  registry already says which properties are rich text. No `RichTextRef` is stored as a
  property value, and the rich-text case leaves `PropertyValue`.
- **The server enforces the registry flags** on every write: `creatable` on Create (a
  non-creatable structure such as DailyNote is made only through `Resolve` by date),
  `name_editable` on Rename, and `unique_names` on Create, Rename and Resolve.
  Unknown structure types are rejected.
- **Name lookup is deterministic.** Names stay non-unique for every structure except
  Tag, the only one with `unique_names`. A lookup by `(structure_type, name)` matches
  case-insensitively and, when several entities match, returns the one with the oldest
  `created_at`, with the lower `id` breaking ties. The rule is documented on `Resolve` in
  the proto, so every client can rely on it.

### Changes to earlier ADRs

- [ADR 1](0001-polymorphic-entity-model.md) calls `uniqueNames` advisory. It is now
  enforced by the server on every write that sets a name.
- [ADR 3](0003-server-authoritative-link-graph.md) names `ResolveByName` as the single
  identity-resolution path. That path is now `Resolve`, with the tie-break rule above.
  ADR 3's rule that no client authors links now holds for `Create` too.
- [ADR 4](0004-dates-without-a-dateref-entity.md) says a DailyNote sets its name at
  creation. The server now sets it on every write, from the `date` property.
- [ADR 7](0007-server-owned-structure-registry.md) leaves name derivation with the
  frontend. Naming is now wholly the server's job; the frontend keeps only presentation.

## Consequences

- **A source-breaking proto change.** Every consumer is in this repo, so it breaks in
  place: each change updates the proto and every consumer (server, frontend, MCP) in one
  task and deletes what it replaces. Nothing is left deprecated, and the branch builds
  and passes its tests after every task.
- One code path per job for every client. The agent and the browser create, resolve and
  move entities through the same RPCs and get the same defaults, names and errors.
- The frontend can't show a new entity before the server replies, because it no longer
  mints the id. This costs nothing today: `NewButton` already waits for the response
  before navigating.
- Registry flags and unknown types fail loudly at the server instead of relying on each
  client to check them.
- Two notes with the same name are still allowed. A wikilink to that name always opens
  the oldest one, which may not be the one the author meant; the author can rename to
  disambiguate.
