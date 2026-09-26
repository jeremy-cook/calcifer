# Calcifer — Issues

Known defects and technical debt in code that exists. New features belong in
[`ROADMAP.md`](ROADMAP.md), not here.

Each entry has a stable ID. When an issue is fixed, move it to **Resolved** with the
date and a one-line note on the fix; don't renumber.

**Severity:** `high` visible bug or data integrity · `medium` bug in an edge case, or
a missing guard · `low` cleanup, performance, tooling.

**Confirmed** means verified by reading the code. **Reproduced** means observed in the
running app. Nothing below has been reproduced yet.

---

## Open

### I-13 · `buf lint` reports RPC naming errors in `services.proto` · low · confirmed

**Where:** `proto/calcifer/v1/services.proto`

**Problem:** `buf lint` (`pnpm proto:lint` in `calcifer/`) fails with 32 findings, all
from the default RPC rules: request/response types not named `<Rpc>Request` /
`<Rpc>Response` (e.g. `ListEntitiesRequest`, `ListStructuresRequest`), and messages such
as `RichText`, `SearchResponse` and `EntityRef` reused as the request or response of
several RPCs. 30 predate I-10; I-10 added two by following the file's existing naming.
Nothing runs `buf lint` today, so this is invisible.

**Fix:** Either rename to the buf convention (a wire-compatible but source-breaking
change for all three consumers), or configure `buf.yaml` to except
`RPC_REQUEST_STANDARD_NAME`, `RPC_RESPONSE_STANDARD_NAME` and
`RPC_REQUEST_RESPONSE_UNIQUE`, documenting the chosen naming convention instead.

**Done when:** `pnpm proto:lint` exits 0.

---

### I-14 · Deleting an entity leaves dead refs in other entities' relation values · medium · confirmed

**Where:** `server/src/services/entity.rs` (`delete`), `calcifer/src/components/entity/EntityRelationsField.tsx`

**Problem:** `Delete` removes `links WHERE target_id = ?` but never strips the deleted id
from other entities' `relation`/`relations` property values. The live database already
has one: a Todo whose `tags` still holds a ref to a Tag that no longer exists.
`EntityRelationsField` hides unresolved refs, but `addRef`/`removeRef` build from the raw
list, so the dead ref is resent on every edit and can't be removed from the UI. It is
harmless today only because relation link sync silently skips missing targets; any
stricter check (e.g. rejecting unknown targets, considered for I-2) would lock such
entities out of every `Update`. Found while doing I-2.

**Fix:** In `delete`, remove the deleted id from other entities' relation property values
in the same transaction (and publish upserts for the entities it changed). Clean up
existing dead refs once; that changes stored data, so confirm with the user first.

**Done when:** after deleting a Tag, no Todo's `tags` value still refers to it, and a test
covers it.

---

### I-15 · Renames and daily-note moves still resend every property · low · confirmed

**Where:** `calcifer/src/model/store.ts` (`useUpdateEntity`), `EntityService.Update`

**Problem:** I-11 moved single-property edits onto `SetProperty`, but renames
(`EntityTitleInput`) and daily-note moves (`DailyNoteDateField`) still use `Update`,
which replaces every property. An `Update` built from a stale copy undoes a concurrent
`SetProperty` (e.g. the agent sets a to-do's status while the browser renames it). The
server test `update_from_a_stale_snapshot_loses_a_concurrent_edit` shows it.

**Fix:** Give renames their own path, e.g. a `Rename` RPC or a field mask on `Update`,
and move daily notes with `SetProperty(date)` plus a rename.

**Done when:** a rename concurrent with a `SetProperty` on the same entity keeps both.

---

### I-16 · Property values aren't checked against their declared kind (except select) · low · confirmed

**Where:** `server/src/services/entity.rs` (`validate_select`), `server/src/link_store.rs`

**Problem:** I-9 checks only select properties. Any other declared property accepts
any value case, e.g. a `relations` value on `content` (declared `richtext`). Relation
link sync would then scope-replace that property's links, which for `content` are the
rich-text links owned by `RichTextService.Put`. No current client sends this. Found
during I-11.

**Fix:** Generalise the per-property check to require the value case to match the
declared `PropertyKind` for every declared property.

**Done when:** `Update`/`SetProperty` with a value of the wrong kind for a declared
property is rejected, and a test covers it.

---

### I-20 · `Entity` is both the write input and the read output · high · confirmed

**Where:** `proto/calcifer/v1/services.proto:68-69` (`CreateEntityRequest`, `UpdateEntityRequest`), `server/src/services/entity.rs:49-51` (`persist_new_entity`), `server/src/services/entity.rs:576` (`update`)

**Problem:** `Create` and `Update` take a full `Entity`, including fields the server
should own: id, links, referenced_dates, created_at and updated_at. `Create` saves the
client-supplied links and referenced_dates (`replace_links` / `replace_referenced_dates`),
which contradicts ADR 3's rule that no client writes links. `Update` silently ignores the
same fields.

**Fix:** Make `Entity` output-only. `CreateEntityRequest { structure_type, optional name,
initial properties }`, with the server minting the id, timestamps and default properties.
Replace `Update` with a `Rename` RPC plus the existing `SetProperty`; this also fixes I-15.

**Done when:** no write RPC accepts an `Entity`, the server mints ids and defaults, and a
test shows a client can't set links or referenced_dates through `Create`.

---

### I-21 · Default entities are built in four places · medium · confirmed

**Where:** `calcifer/src/model/store.ts:21-85` (`defaultNameFor`, `buildEntityMessage`, `buildDailyNoteMessage`), `server/src/services/entity.rs:406-474` (`build_resolved_entity`, `build_daily_note`), `calcifer/src/routes/e.$id.tsx:166`

**Problem:** The frontend and the server each build new entities (rich-text refs, select
defaults, the DailyNote date and name) with separate code; the server's
`build_resolved_entity` says it "mirrors the FE's buildEntityMessage". The default name
`Untitled X` is set in `store.ts:22` and compared again in `e.$id.tsx:166`. The frontend
mints the id itself "so the editor has its richtext refs immediately", but `NewButton`
(`calcifer/src/layouts/sidebar/NewButton.tsx:27-28`) waits for the server's response
before navigating anyway, so this gains nothing.

**Fix:** Fixed by I-20: the server is the only place that builds a new entity, and the
frontend builders go away.

**Done when:** `buildEntityMessage` and `buildDailyNoteMessage` are gone, and the default
name is defined only on the server.

---

### I-22 · A rich-text ref stored as a property value adds nothing · medium · confirmed

**Where:** `proto/calcifer/v1/entities.proto:23` (`PropertyValue.richtext`), `calcifer/src/routes/e.$id.tsx:209`, `calcifer/src/components/calendar/DailyNoteSection.tsx:91,148-152`

**Problem:** A `RichTextRef` is just (entity.id, property.id), both already known from
context, and the registry already says which properties are rich text. Because the value
is stored anyway, it has to be created on every new entity (I-21), the server never
checks that its `entity_id` matches the owning entity, and if the row is missing the
editor renders nothing.

**Fix:** Drop the `richtext` case from `PropertyValue` and address a document by
(entity_id, declared property_id).

**Done when:** no `richtext` property value is stored or sent, and the editor renders for
every declared rich-text property.

---

### I-23 · Daily notes are created and moved differently by each client · medium · confirmed

**Where:** `calcifer/src/model/store.ts:124-130,271-273` (`useCreateDailyNote`, `withDailyNoteDate`), `mcp-server/src/tools.ts:113-126` (`resolveDailyNote`), `server/src/services/entity.rs:687-698` (`set_property`), `server/src/structures.rs:126`

**Problem:** The frontend creates daily notes with `Create`, naming them with
`formatLongDate` on the client, and moves them with `Update` plus a client-side rename.
The MCP server calls `CreateDailyNote` and, on `AlreadyExists`, lists every DailyNote and
scans their properties. `SetProperty(date)` updates `date_key` but not the name, so a
client using the single-property path ends up with a name that no longer matches the
date. The server doesn't enforce `name_editable = false` either.

**Fix:** The server derives a DailyNote's name from its date on every write. Merge
`ResolveByName` and `CreateDailyNote` into one get-or-create RPC:
`Resolve { oneof key { name, date }, create_if_missing } → { entity, created }`. Moving a
daily note then becomes a plain `SetProperty(date)`.

**Done when:** `SetProperty(date)` on a DailyNote also renames it, both clients get-or-create
daily notes through `Resolve`, and tests cover both.

---

### I-24 · Looking up an entity by name isn't reliable for most structures · medium · confirmed

**Where:** `server/src/services/entity.rs:61-77` (`find_by_name`), `server/src/services/entity.rs:349-352` (`validate_select`), `server/src/structures.rs:110`

**Problem:** Only Tag has `unique_names`, but `[[wikilinks]]`, @ mentions and the MCP
`get_note` all look up Notes and Todos by name through `ResolveByName`. `find_by_name`
uses `LIMIT 1` with no `ORDER BY`, so which of two same-named notes you get is undefined.
`structure_type` isn't checked (the registry check skips unknown structures), so an empty
or unknown type creates an entity anyway.

**Fix:** Either make names unique wherever lookup by name is used, or make the lookup
deterministic and document it. Reject unknown structure types.

**Done when:** a lookup with two same-named entities returns the same one every time (or
the duplicate can't exist), a create with an unknown or empty structure type is rejected,
and tests cover both.

---

### I-25 · Wrong error message on a unique-name clash · low · confirmed

**Where:** `server/src/services/entity.rs:523,567` (`create`, `update`), `server/src/services/entity.rs:397` (`map_unique_violation`)

**Problem:** `Create` and `Update` map every unique violation to "a DailyNote for this
date already exists". Creating or renaming a Tag onto an existing Tag name (which hits
`one_tag_per_name`) gets that message.

**Fix:** Pick the message from the constraint that failed (`one_daily_note_per_day` vs
`one_tag_per_name`).

**Done when:** a Tag name clash returns `AlreadyExists` with a Tag message, and a test
covers it.

---

### I-26 · Unused property kinds, and structure flags only the frontend enforces · low · confirmed

**Where:** `server/src/structures.rs` (registry), `calcifer/src/routes/e.$id.tsx:257`

**Problem:** No structure declares a `relation`, `text` or `number` property, and the
entity page silently renders nothing for them (`default: return null`). The server
doesn't enforce `creatable` or `name_editable`; only the frontend does.

**Fix:** Either remove the unused kinds or render them. Enforce `creatable` and
`name_editable` on the server once I-20 and I-23 have landed.

**Done when:** every `PropertyKind` is either used and rendered or removed, and the server
rejects creating a non-creatable structure and renaming a non-name-editable entity.

---

### I-27 · Watch isn't built for keeping a full copy in sync · high · confirmed

**Where:** `calcifer/src/App.tsx:11-43` (`useWatchSync`), `server/src/services/entity.rs:858-870` (`watch`), `server/src/watch.rs:24`, `calcifer/src/model/store.ts:109,172,241`

**Problem:** The frontend keeps a full copy of every entity (reasonable for one user, ADR
5), but Watch doesn't support that:
- Race at startup: the initial `List` and the Watch subscription start at the same time
  (`App.tsx:14-21`), so events that land between them are lost.
- Lost events go unnoticed: the server drops events for a subscriber that falls behind
  the 256-slot channel (`entity.rs:863-868`), and nothing tells the client to reload.
- Reconnects: after a reconnect (`App.tsx:32-36`) the client doesn't refetch the list.
- Refetch storm: every event and every mutation invalidates `['entities']`, so the
  frontend reloads the full list each time, which the server serves as 1 + 4N queries.
  Each debounced keystroke save means a full reload, and each `SetProperty` means two
  (one from `onSettled`, one from the Watch echo).

**Fix:** Make Watch start with a snapshot (or accept a `since` revision) and carry a
revision number on each event. When a subscriber falls behind, send an explicit resync
signal instead of dropping events. Have the frontend apply `Upserted`/`Deleted` directly
to the cached list instead of refetching. State that the filtered `List`,
`ListBacklinks` and `Search` are there for the agent.

**Done when:** no event is lost between the initial load and the subscription, a lagging
or reconnecting client resyncs, and an edit no longer triggers a full list refetch.

---

### I-28 · `repeated Property` should be `map<string, PropertyValue>` · medium · confirmed

**Where:** `proto/calcifer/v1/entities.proto:55` (`Entity.properties`), `calcifer/src/model/store.ts:188,332`, `calcifer/src/model/todos.ts:42,49`, `calcifer/src/components/calendar/DailyNoteSection.tsx:149`, `calcifer/src/routes/e.$id.tsx:204`, `mcp-server/src/tools.ts:121`

**Problem:** Every consumer repeats `properties.find(p => p.id === id)?.value?.value`, and
nothing stops two properties with the same id. `Property` is `{id = 1, value = 2}`, which
is exactly how protobuf encodes a map entry.

**Fix:** Change `properties` to `map<string, PropertyValue>`. The change is
wire-compatible and only breaks source code; a map also rules out duplicate ids.

**Done when:** `Entity.properties` is a map and no consumer scans a property list.

---

### I-29 · Search has two RPCs for one job · low · confirmed

**Where:** `proto/calcifer/v1/services.proto:39,42` (`Search`, `Retrieve`), `server/src/services/search.rs:62`, `mcp-server/src/tools.ts:74-87`

**Problem:** `Search(query, limit)` and `Retrieve(query, k, hybrid)` return the same
response, and `Retrieve` falls back to lexical search anyway. Separately, snippets mark
matches with `[` `]`, which clashes with `[[wikilink]]` syntax.

**Fix:** One `Search(query, limit, mode)` with a `SearchMode` enum; the MCP server
already models exactly this (`tools.ts:74`). Return match ranges instead of bracketed
snippets.

**Done when:** there is one search RPC with a mode, and hits carry match ranges instead of
`[ ]` markers.

---

### I-30 · `ListBacklinks` takes the wrong request and returns too little · low · confirmed

**Where:** `proto/calcifer/v1/services.proto:24`, `server/src/services/entity.rs:815-835` (`list_backlinks`), `mcp-server/src/tools.ts:35`, `calcifer/src/model/backlinks.ts:40`

**Problem:** It takes an `EntityRef` but reads only `id`, so the MCP server fills in
`structureType: ''`. It returns plain entities with no link details (which property the
link came from, or when). The frontend never calls it and works out backlinks itself,
and the two disagree: the frontend excludes an entity linking to itself, the server
doesn't.

**Fix:** Use `ListBacklinksRequest { entity_id }`, or state that this RPC is only for the
agent (see I-27). Make both paths agree on self-links.

**Done when:** `ListBacklinks` takes a request with only `entity_id` (or is documented as
agent-only), and the server and frontend agree on self-links.

---

### I-31 · The API contract is undocumented · low · confirmed

**Where:** `proto/calcifer/v1/entities.proto`, `server/src/links.rs:30,52-70`, `mcp-server/src/markdown/` (TipTap conversion)

**Problem:** The proto doesn't say that date values are ISO `yyyy-MM-dd` (only
`CreateDailyNoteRequest` does), that links, referenced_dates and `RichText.updated_at`
are output-only, or that relation values only need the target id. `RichText.doc` is
TipTap JSON that the server parses, so the editor's node types (`mention`, `hashtag`,
`dateChip` and their attributes) are part of the API, but the proto says nothing about
them, and the MCP server had to reimplement them.

**Fix:** Document these in the proto comments and in `docs/reference/data-model.md`,
including the rich-text node types the server reads.

**Done when:** every field above has a proto comment, and the rich-text node types the
server parses are documented.

---

### I-32 · Proto and Connect details leak into components · low · confirmed

**Where:** `calcifer/src/routes/e.$id.tsx:251`, `calcifer/src/components/entity/EntityRelationsField.tsx:2,37`, `calcifer/src/components/calendar/DailyNoteDateField.tsx:2,19`, `calcifer/src/model/api.ts:28`

**Problem:** Components build proto messages directly (`createMessage(EntityRefListSchema,
…)`, `createMessage(EntityRefSchema, …)`) and branch on `ConnectError` codes, instead of
going through the model layer.

**Fix:** Keep reading proto types in components, as ADR 2 intends, but route writes
through model helpers (`setRelations(entity, id, ids)`, `setDate`, `setSelect`) and add
`isAlreadyExists` next to `isNotFound` in `api.ts`.

**Done when:** nothing under `components/` or `routes/` imports `@bufbuild/protobuf` or
`@connectrpc`.

---

### I-33 · Duplicated code in the model layer · low · confirmed

**Where:** `calcifer/src/model/backlinks.ts:14`, `calcifer/src/model/store.ts:316`, `calcifer/src/model/todos.ts:117`, `calcifer/src/App.tsx:16`, `calcifer/src/model/store.ts:92,112-130,275-312`

**Problem:**
- Timestamp-to-milliseconds conversion is written three times; `timestampMs` from
  `@bufbuild/protobuf/wkt` already does this.
- The `List` query function is written twice (`App.tsx:16`, `store.ts:92`).
- The delete path (`useDeleteEntity`, `deleteEntityImperative`) and the create mutation
  (`useCreateEntity`, `useCreateDailyNote`) each exist twice.
- A raw `['entities']` key is used in places instead of `qk`.
- The Watch consumer lives in `App.tsx` instead of `model/`.

**Fix:** Use `timestampMs`. Export an `entitiesQuery` the way `structuresQuery` is
exported. Merge the duplicate delete and create paths, use `qk` everywhere, and move the
Watch consumer into `model/`.

**Done when:** each of the above exists once, and no raw `['entities']` key remains.

---

### I-34 · Relation edits resend the whole list · low · confirmed

**Where:** `calcifer/src/components/entity/EntityRelationsField.tsx:32-40`, `calcifer/src/routes/e.$id.tsx:246-252`

**Problem:** Adding or removing a tag sends the full relations list through
`SetProperty`. If the browser and the agent tag something at the same time, one change is
lost. Related to I-14.

**Fix:** Add and remove single relation targets (e.g. add/remove ops on `SetProperty`)
instead of replacing the list. Deferred: revisit if the agent starts tagging.

**Done when:** concurrent tag additions from two clients both survive.

---

### I-36 · `ResolveByName` returns a generic error when it loses a create race · low · confirmed

**Where:** `server/src/services/entity.rs:803-804` (`resolve_by_name`)

**Problem:** `ResolveByName` with `create_if_missing` looks the name up, then creates.
If a concurrent call creates the same unique-named entity (a Tag) in between, the
insert hits `one_tag_per_name` and is mapped with `Status::from`, not
`map_unique_violation`, so the caller gets a generic database error instead of the
winner. Found during T01 of the API review.

**Fix:** On a unique violation, re-read by name and return the winner with
`created = false`. T07's `Resolve` replaces this path and must do this for both keys.

**Done when:** a get-or-create that loses the race returns the existing entity, and a
test covers it.

---

### I-37 · A racing `RichText.Put` fails with `Internal`, not `FailedPrecondition` · medium · confirmed

**Where:** `server/src/services/richtext.rs` (`put`, `self.pool.begin()`), `server/src/db.rs:35-43`

**Problem:** `Put` reads the stored `updated_at` and then writes, inside a DEFERRED
transaction. The pool runs WAL with a `busy_timeout`, but a deferred transaction that
has read can't upgrade to a writer once another connection has committed since its read
began. SQLite returns `SQLITE_BUSY` at once, and the timeout doesn't help. So when two
`Put`s race, the loser gets `Internal` instead of `FailedPrecondition`. No write is lost,
but clients that react to a conflict (the MCP append retry, the editor's "changed
elsewhere" reload) don't see one. Found during T04 of the API review; confirmed by
reading, not reproduced.

**Fix:** Start `Put`'s transaction with `BEGIN IMMEDIATE` so the write lock is taken
before the read, or map a busy error on this path to `FailedPrecondition`.

**Done when:** two concurrent conditional `Put`s on the same doc give one success and
one `FailedPrecondition`, and a test covers it.

---

### I-38 · Stale comment about which writes publish events · low · confirmed

**Where:** `server/src/watch.rs:2`

**Problem:** `watch.rs` says only Create/Update/Delete publish events; `RichText.Put`
now publishes too. Found during T04 of the API review. (The matching stale comment in
`calcifer/src/model/richtext.ts` was removed in T06.)

**Fix:** Update the comment. T14 of the API review rewrites `watch.rs` and can pick this
up.

**Done when:** the comment doesn't contradict the code.

---

### I-39 · Leaving a new daily note within the save debounce deletes what was typed · medium · confirmed

**Where:** `calcifer/src/components/calendar/DailyNoteSection.tsx:45-54` (prune on unmount), `calcifer/src/components/entity/EntityRichTextField.tsx` (`RICHTEXT_DEBOUNCE_MS` timer)

**Problem:** The editor saves 300 ms after the last keystroke. If you type into an empty
daily note and leave the day inside that window, the prune's server check still sees an
empty doc and deletes the note. The pending save then fires against a deleted entity
and fails, so the typed text is lost. The old cache-based check had the same race.
Found during T06 of the API review; confirmed by reading, not reproduced.

**Fix:** Flush the pending save when the editor unmounts, and have the prune wait for
that doc's save queue to settle before checking the server.

**Done when:** typing into a new daily note and navigating away immediately keeps the
note and its text.

---

## Resolved

- **I-17 · `RichText.Put` sends no Watch event and has no conflict check.** Fixed 2026-09-25. `Put` checks the entity and declared property, takes `expected_updated_at` (mismatch is `FailedPrecondition`) and publishes `rich_text_changed` plus `upserted`; covered by server tests. The MCP append sends the expectation and retries on conflict. The browser writes Watch's `rich_text_changed` to the cache, loads newer versions into an open editor when no local save is pending, sends `expected_updated_at` on serialised saves, and on a conflict reloads the server's doc with an inline notice (D3, no merge). Browser behaviour observed 2026-09-25 against a scratch DB: live append, conflict reload with notice, prune kept a note the agent wrote to.
- **I-18 · `RichText.Get` uses `NotFound` for "nothing saved yet".** Fixed 2026-09-25. `Get` returns an empty doc at the epoch for a declared, unsaved property and `NotFound` only for a missing entity or undeclared property; neither client treats `NotFound` as an empty doc any more.
- **I-35 · Pruning an empty daily note can delete content the agent just appended.** Fixed 2026-09-25. The prune fetches the doc from the server (`deleteEntityIfRichTextEmpty`) and deletes only if it's empty; a write landing between that Get and the Delete can still be lost. Not yet observed in the browser.
- **I-19 · Daily notes are sorted by name, not date.** Fixed 2026-09-25. `listByStructure` sorts DailyNotes by their ISO `date` property, newest first; undated notes sort last and ties break by `id`. Verified by reasoning and a script check of the comparator; not observed in the browser.
- **I-8 · Updating a missing entity returns a foreign-key error.** Fixed 2026-09-24. Update checks `rows_affected()` and returns `NotFound` for an unknown id; covered by `update_missing_entity_is_not_found`.
- **I-3 · `Update` can change an entity's `structure_type`.** Fixed 2026-09-24. Update no longer writes `structure_type`; a mismatch with the stored type returns `InvalidArgument`; covered by `update_rejects_structure_type_change`.
- **I-1 · Title input can revert mid-typing.** Fixed 2026-09-24. Renames are debounced (300 ms) and flushed on blur/unmount, and `entity.name` is not synced into the input while it is focused. Verified by reasoning; not reproduced in the browser.
- **I-12 · Eight ESLint errors on `main`.** Fixed 2026-09-24. ESLint config turns off `only-export-components` for `src/routes/**` and `src/components/ui/**`; `prefer-const` fixed by hand. `pnpm lint` exits 0.
- **I-6 · Concurrent optimistic updates can undo each other on rollback.** Fixed 2026-09-24. Rollback replaces only the failed entity (list row and detail cache), never the whole list snapshot. Verified by reasoning.
- **I-5 · A daily-note move the server rejects fails silently.** Fixed 2026-09-24. `useUpdateEntity` takes an `onError`; `DailyNoteDateField` shows an inline error on `AlreadyExists`. No shared toast (no toast library). Verified by reasoning.
- **I-4 · To-do "today" doesn't roll over at midnight.** Fixed 2026-09-24. `useToday()` in `model/dates.ts` re-renders at local midnight and re-checks on focus/visibility; used by the to-do list, rows and calendar route. Midnight arithmetic exercised by script (incl. DST); clock change not observed in the browser.
- **I-7 · `TodoRow` subscribes to the whole entity list.** Fixed 2026-09-24. `TodoList` resolves tags through one memoized `Map` and passes them in; `TodoRow` is memoized and no longer calls `useAllEntities()`.
- **I-10 · The structure registry is defined three times.** Fixed 2026-09-24. Registry authored only in `server/src/structures.rs`, served by `StructureService.List` (ADR 7); frontend keeps icons/colors and fetches the rest; MCP `list_structures` reads the RPC. Verified by adding a priority option to `structures.rs` alone: server, frontend and MCP all build and test clean. Generated TS now carries `// @ts-nocheck` because proto enums aren't erasable syntax.
- **I-9 · Select values aren't checked against the allowed options.** Fixed 2026-09-24. Create/Update check every property against the registry: a declared select must hold one of its options, and a select value is only allowed on a declared select. Covered by `update_rejects_unknown_select_value` and three more tests.
- **I-2 · Relation targets aren't checked against `targetStructure`.** Fixed 2026-09-24. Relation link sync loads each target's real type and rejects a mismatch with the declared `target_structure` or with the ref's claimed type; links store the real type. Refs to deleted targets are still accepted (see I-14). Covered by `update_rejects_note_in_todo_tags` and three more tests.
- **I-11 · Property edits send the whole entity (last write wins).** Fixed 2026-09-24. New `EntityService.SetProperty` writes one property row and syncs only that property's links, reusing the I-9 select check and I-2 relation checks; the frontend's `useSetProperty` (per-property optimistic rollback) now backs to-do status and the entity page's property fields. Covered by `concurrent_set_property_edits_both_survive` and six more tests. `Update` can still overwrite properties (see I-15).

Fixed on 2026-09-24 from the to-do / calendar review.

- **R-1 · Removing the last tag left a stale backlink.** `sync_relation_links` now goes
  through the structure's declared relation properties (`structures.rs`
  `relation_properties`) and clears the links for any that are absent.
- **R-2 · Priority/status options defined in four places.** Now derived from
  `STRUCTURES.Todo` via `propertyDef()`. The Rust defaults remain; see I-10.
- **R-3 · Filter/sort values repeated three times.** Defined once in `todos.ts`, with
  types, validation and toolbar all derived from them. The redundant `due=any` value was
  dropped.
- **R-4 · Three copies of the update logic; optimistic update skipped the list.** Merged
  into `useUpdateEntity`, which updates both caches, plus the pure builders
  `withName` / `withProperty` / `withDailyNoteDate`.
- **R-5 · `EntityDateField` had a daily-note default message and a `void | boolean`
  `onChange`.** Replaced by `validate?: (iso) => string | null`. The message now lives in
  `DailyNoteDateField`.
- **R-6 · "This week" filter off by a day across daylight saving.** Uses `shiftIso`.
- **R-7 · Unused `cn` package.** It was actually imported by mistake in `badge.tsx` and
  `checkbox.tsx`. Imports now point to `~/lib/utils`, and the package is removed.
- **R-8 · Entity page special-cased DailyNote and Todo status by name.** Replaced with an
  override map in `EntityProperties`.
- **R-9 · Link persistence lived in `services/entity.rs`.** Moved to
  `server/src/link_store.rs`.
- **R-10 · `/s/Todo` silently overrode `/s/$structureType`.** Comment added.
