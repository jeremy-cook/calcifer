# 9. Views query the server; Watch is a change feed

- **Status:** Accepted
- **Date:** 2026-11-07. Rewritten in place: until then this record decided the opposite,
  that the frontend is a full replica fed by Watch (2026-09-25). That text is in git at
  `477b492`.

## Context

The earlier version of this record kept a full copy of every entity in the browser, fed by
`WatchEntities`: each stream opens with a snapshot of the whole `entities` table (every
structure, every property, every outgoing link, every referenced date), then streams
upserts and deletes. The browser derives everything else from that copy: backlinks, the
calendar, ToDo filtering, the @ picker, ⌘K titles.

That version called this "fine at single-user sizes" and named data size as the first thing to
revisit. Every cost it carries grows with the whole dataset, not with what is on screen:

| Cost | Paid | Grows with |
|---|---|---|
| The snapshot on the wire | Every page load, reconnect and lagging tab | Every entity and every link |
| The replica in memory | Always | Every entity, as protobuf objects |
| Rebuilding `byId` and `all` (`model/replica.ts`) | Every write, including each debounced save | Every entity |
| Derived views (calendar index, incoming links, ToDos) | Every write, per mounted view | Every entity × its links |

A link is about 100 bytes on the wire, so an entity with ten links is 1–3 KB. At 5,000
entities the snapshot is 5–15 MB; at 50,000 it is 50–150 MB per reconnect, and every save
rebuilds indexes over the whole graph. (Estimates, not measurements.) Agents write at
volume: one Space Race research run made 230 notes. Tens of thousands of entities is a
size to plan for.

Nothing in the browser needs the whole set. Every reader of the replica wants a bounded
slice:

| Reader | Reads today | Needs |
|---|---|---|
| Notes list, Tags A–Z (`routes/s.$structureType.tsx`) | Every entity, filtered by type | A page of one structure |
| Note page, relationships section, tag page (`routes/e.$id.tsx`, `model/relationships.ts`) | Every entity, scanned for links in | One entity, its outgoing targets and its backlinks |
| Calendar, Day and Week views (`model/calendar.ts`) | An index over every entity | The items in a date range |
| Home (`HomePage.tsx`) | Every entity | Today's daily note, ToDos due this week, the latest notes |
| ToDo page (`model/todos.ts`, `TodoToolbar.tsx`) | Every ToDo and every Tag | The ToDos matching the filter; tag names for the menu |
| @ picker, relation picker (`model/mentions.ts`, `EntityPicker.tsx`) | Every name | The top matches for what's typed |
| ⌘K titles (`model/search.ts`) | Every name | The top matches |
| Chip labels (`MentionNodeView.tsx`) | One entity by id | The entities the open document links to |
| Daily-note lookups (`model/daily-notes.ts`) | Every entity, scanned by date | One entity by date (`ResolveEntity` already does it) |

The server runs on the same machine, so a round trip costs a few milliseconds.

The replica itself replaced an earlier list-plus-invalidation design that had a startup race, lost
events silently, and stormed the server with full reloads (1 + 4N queries per event). Those
failures came from reloading everything on every event and from not ordering queries
against the stream, not from querying per view. This design has to avoid both.

## Decision

**Each view asks the server for its slice. The browser holds only what views have loaded.
`WatchEntities` stops sending snapshots and becomes a change feed that tells the browser
which loaded data is stale.**

### Reads are view-shaped

Writes stay intent-shaped ([ADR 8](0008-server-builds-entities.md)). Reads are bounded and
generic: they filter by structure, facet and property id, never by a structure's meaning,
so a new structure or facet needs no new RPC.

| Read | Shape | Serves |
|---|---|---|
| `ListEntities` (extended) | `structure_type?`, `facet?`, property filters (equals, one of, date range), an order (name, `updated_at`, `created_at` or a property), `page_size`, `page_token`, `read_mask` → entities, `next_page_token` | Lists, Home, the ToDo page |
| `GetEntities` (new) | ids, `read_mask` → entities | Chip labels, outgoing relationship targets |
| `ListBacklinks` (exists) | entity id → backlinks | Relationships section, tag page |
| `CountReferences` (new) | target ids → distinct referring entities per id | Tag counts on the Tags list |
| `MatchNames` (new) | query, structure types, limit → entities | @ picker, relation picker, ⌘K titles. Matches a prefix of I-83's folded name key (`fold_name`). |
| `ListCalendar` (new) | start and end day → items (day, kind, entity id) and their entities. The days are daily-note dates, referenced dates, created days, and the values of date properties the registry flags for the calendar. | Calendar views, Home |
| `ResolveEntity`, `Search`, `GetRichText` (exist) | unchanged | Daily-note lookup, text search, documents |

Relationship rows are still built and ordered in the browser by `model/relationships.ts`
(ADR 12's ordering), from the entity, its outgoing targets and its backlinks.

**Paging is a forward cursor**, named as in AIP-158. A request sends `page_size` and the
`page_token` from the previous response (none for the first page); the response sends
`next_page_token`, empty on the last page. The token is opaque and encodes the order's key
and the id, so a page doesn't shift when something is created or deleted earlier in the
list, as an offset would. There is no backward cursor: the browser keeps the token for
each page it has turned to, so going back refetches with that page's token. There is no
total either; nothing needs one yet.

**Field masks choose what a read returns**, named as in AIP-157. `read_mask` is a `google.protobuf.FieldMask`
over `Entity`: `name`, `properties`, `properties.todo.due`, `links`, `facets`, and so on.
`id` and `structure_type` always come back; no mask means every field.

How the list pages load:

- **Structure lists come a page at a time**, with Previous and Next controls. `]` goes to
  the next page and `[` to the previous one, except while typing in a field. Pages
  already seen stay cached, so going back is instant.
- **The Tags A–Z index loads every tag at once**, with `read_mask: name`: about 50 bytes a
  tag, so 500 KB at 10,000 tags. It keeps its three-column layout. Tag counts come from
  `CountReferences` for the tags on screen.

### Storage and registry the reads need

- **Queryable property values.** `properties.value_blob` is an encoded `PropertyValue`, so
  SQL can't filter ToDos by `todo.due` or `todo.status`. `properties` gains a `value_key`
  column: canonical sortable text for scalar kinds (an ISO day, a select key), null for
  the rest, written in the same transaction as the blob and indexed on
  `(property_id, value_key)`. A migration backfills it.
- **List order.** An index on `entities(structure_type, updated_at)`.
- **A calendar flag in the registry.** `PropertyDef` gains `on_calendar`, allowed only on
  date properties. `todo.due` sets it, replacing the browser's hard-coded `due` kind, so a
  new date property reaches the calendar only when its definition says so.
- `links(target_id)` and `referenced_dates(iso_date)` are already indexed, so backlinks
  and calendar ranges cost what their result costs.

### Watch is a change feed

`WatchEntitiesResponse` becomes:

- `ready`: the first message on every stream. No snapshot.
- `changed { Entity entity; }`: replaces `upserted`. The entity as it is now.
- `deleted { EntityRef ref; }`: replaces `deleted_id`. It carries the structure as well as
  the id, so lists of that structure know they're affected.
- `rich_text_changed`: unchanged.
- `resync`: sent instead of the missed events when a subscriber lags.

Revisions stay, increasing within a stream. Query responses don't carry one (see the
freshness rules).

### An event is about one entity as it is now

A view is built from the entities it shows, so it is refreshed from them too: no view
needs history its first load didn't. Events never carry what an entity used to be.

An entity's state includes the links pointing at it, not only the ones it writes. A write
that adds or removes a link changes both ends, so the server publishes a `changed` for the
source and for every target it gained or lost. When the agent removes `[[Mercury]]` from
*Apollo 7*, the server publishes *Apollo 7* and Mercury, and Mercury's open page refetches
its backlinks. `replace_scoped_links` already reads the old targets before rewriting
them, so finding them costs nothing, and `DeleteEntity` already republishes the entities
whose relationships it removes.

### The browser's cache

- **Entities live in the TanStack Query cache, one entry per id.** View queries return
  entities; the model layer seeds each into its per-id entry and the view reads rows
  through those entries. A rename or an edit then reaches every view showing that
  entity, chips included, from one write.
- **A masked read never seeds an entry.** Its entities are partial, so they stay in that
  query's result. An entity's entry always holds the whole entity.
- `model/replica.ts` is deleted. `model/sync.ts` becomes the freshness module below.
- Mutations write the server's response into the entity's entry, with optimistic updates
  and per-entity rollback as now. The `updated_at` guard against stale echoes (I-49)
  moves with it.

### Freshness rules

1. **Subscribe first.** The browser opens Watch and runs no view query until `ready`, so
   any write that commits after a query has read reaches the browser as an event.
2. **Reconnect or `resync`: mark every query stale.** Active ones refetch. That costs the
   views on screen, not the database.
3. **On `changed` or `deleted`:** write the entity into its entry (or remove the entry),
   then mark stale every query that is about the entity, whose result holds it, or whose
   filter it now matches. Active ones refetch, coalesced to one pass per ~50 ms, so an
   agent's burst of writes costs one refetch per view.
4. **A change during a fetch restarts the fetch** (TanStack's `cancelRefetch`), so a
   response read before a change never lands after it.

Rule 3 per query, in one module:

| Query | Stale on a change to entity E when |
|---|---|
| E's entry | Never; it is written directly |
| A list page | The page holds E (it may have left or moved), or E now matches the list's filters (it may have joined) |
| Backlinks of X, `CountReferences` for X | E is X (published because a link to X changed), or the result holds E (a source renamed or deleted) |
| A calendar range | The result holds E, or one of E's days now falls in the range |
| Name matches, search | Not refreshed live; they're short-lived and refetch as you type |

**Kept from the replica design:** `PutRichText`'s conflict check, rich-text events, and the server's
version winning a lost save, with the "changed elsewhere" note.

### Changes to earlier ADRs

- [ADR 5](0005-single-user.md) calls Watch "a fan-out broadcast, not a sync protocol". It
  is that again.
- [ADR 8](0008-server-builds-entities.md) shapes writes by intent; this ADR shapes reads by
  view.
- CLAUDE.md's **State** section changes: entities are data the browser requests, in
  TanStack Query; Watch only marks it stale. It changes in rollout step 6, with the code.

## Alternatives considered

- **Keep the full replica and make it cheaper.** Resume a stream from a revision with a
  short server-side event log, and cache the replica in IndexedDB. Fixes the wire cost on
  reconnect and load, but memory and per-write work still grow with the whole dataset.
  It postpones the limit.
- **Replicate a slim header** (id, structure, name, `updated_at`) for the picker, chips and
  ⌘K, and query the rest. About 100 bytes an entity, so 5 MB at 50,000. Not needed at local
  latency, and it is still a whole-dataset cost on every load and write.
- **Refetch every active query on every change.** No mapping table at all. Rejected as the
  design because every debounced save would refetch every open view, but it is the
  fallback if the mapping proves fiddly, and a safe first step.
- **Live queries on the server**, which tracks each client's open queries and pushes
  results. Moves per-client state into the server for a single user; no.
- **A Zustand store of loaded entities** (keep `replica.ts` as a partial cache). Workable,
  but it holds data the browser requested, which belongs in TanStack Query.
- **Events that carry what an entity was** (its whole old version, or the link targets and
  days a change touched), so the browser can work out which views a source's change
  affects. Not needed once the server publishes each link target as changed: every view
  is then refreshed from the entities it shows, as it was first loaded.
- **GraphQL's connection model** (`first`/`after`, `last`/`before`, `page_info`, a total).
  Only Next needs a cursor, since Previous returns to a page already turned, and nothing
  shows a total yet.
- **Offset paging, or infinite scroll.** Offsets shift when something is created or
  deleted earlier in the list; cursors don't. Pages with a shortcut keep a place in a
  long list better than an endless scroll.

## Consequences

- Memory and per-write work in the browser grow with what is on screen. A reconnect
  refetches the open views, not the database.
- **Views load.** A first visit to a page waits for a round trip; the placeholders from
  65abcc9 cover it. Revisiting a page shows the cached result while it refetches.
- **Offline shows less.** With the server down, only views already cached render, and
  the picker can't match names. The connection pill already says the server is
  unreachable.
- Browser and agent share reads. `ListEntities`, `ListBacklinks` and `Search` get a browser
  caller, so a regression in them shows up in the app, which the replica design called
  out as missing.
- Calendar indexing and ToDo filtering move into SQL. Grouping ToDos (Overdue, Today, This
  week…) stays in the browser, over the filtered set. The open groups (Overdue through No
  date) load every matching ToDo, which open ToDos keep bounded. Done grows without limit,
  so it pages by cursor, newest first, like a structure list.
- A write that changes links publishes one event per target gained or lost, besides its
  own. A save that adds 30 chips publishes 31 events; the browser coalesces their
  refetches.
- A migration (`value_key`, its backfill, the list index) and a source-breaking proto
  change, which both clients take in place.
- The "Needs new data" items in `plans/redesign/` become reads: tag counts are
  `CountReferences` and overdue/upcoming grouping uses the ToDo query. Totals and
  sidebar counts wait until they're wanted, and then get a count read of their own
  rather than a field on every page.

## Rollout

Each step leaves the app working, as ADR 8's did.

1. **Measure the baseline.** Seed scratch databases with 5,000, 20,000 and 50,000 entities
   and record snapshot size, time until the first view renders, heap size, and the cost
   of one save. The same script is the acceptance check at the end.
2. **Server reads**, with tests: `value_key` and its migration, the `on_calendar` flag,
   the extended `ListEntities`, `GetEntities`, `CountReferences`, `MatchNames`,
   `ListCalendar`.
3. **Watch** sends `ready`, `changed`, `deleted` and `resync` beside the snapshot, and
   publishes link targets as changed, so the current browser keeps working.
4. **Browser cache and freshness module**, beside the replica.
5. **Views move one at a time:** the note page and relationships, chips, the pickers and
   ⌘K, lists, the calendar, ToDos, Home.
6. **Remove** the snapshot, `upserted`, `deleted_id`, `model/replica.ts` and the
   whole-set derivations, and update CLAUDE.md and `docs/reference/data-model.md`.

## Decisions while drafting

- Lists come in pages with a forward cursor and no total; `]` and `[` turn them.
  (2026-11-04)
- Reads take a field mask; the Tags A–Z index loads names only with one. (2026-11-04)
- Date properties reach the calendar by an `on_calendar` registry flag. (2026-11-04)
- An event carries an entity as it is now. A link change publishes its targets too.
  (2026-11-04)
- The ToDo page loads its open groups in full and pages Done. (2026-11-07)
- This record replaces the full-replica decision in place, keeping number 9, rather than
  superseding it with a new record. (2026-11-07)

## Notes

- **2026-11-01, from the earlier version:** the search palette's "In the text" results call
  `Search` in lexical mode only. Vector search returns its nearest notes however
  unrelated, so meaning matches read as noise there; agents keep hybrid search over MCP.
  The palette's title matches came from the replica and move to `MatchNames`.
