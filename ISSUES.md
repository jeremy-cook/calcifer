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

**Deferred:** Calcifer is a single-user local app, so this race is very unlikely. Revisit if it's ever seen.

**Done when:** a get-or-create that loses the race returns the existing entity, and a
test covers it.

---

### I-42 · A rich-text save that fails for a non-conflict reason is dropped silently · medium · confirmed

**Where:** `calcifer/src/model/richtext.ts` (`putOnce`, the non-`FailedPrecondition` branch)

**Problem:** If a `Put` fails for any reason other than a conflict (the server is
down, a network error), the saver logs to the console and moves on. The editor shows
no error and doesn't retry, so the text is lost unless another edit follows once the
server is back. If that editor was a daily note being left, the prune then sees an empty
doc and deletes the note. Found during T06b of the API review; confirmed by reading.

**Fix:** Keep the unsaved doc and retry with backoff, and show an inline "not saved"
notice (the same slot as the conflict notice) until a save succeeds. The prune should
skip the delete while a doc has an unsaved local change.

**Deferred:** Calcifer is a single-user local app, so this is very unlikely to happen. Revisit if it's ever seen.

**Done when:** typing while the server is down shows the notice, and the text is saved
once the server returns without further typing.

---

### I-45 · Two untitled Tags created at once can pick the same free name · low · confirmed

**Where:** `server/src/services/entity.rs` (`create`, `free_name`)

**Problem:** `free_name` looks up a free `Untitled Tag N` outside the insert
transaction. Two Creates of an untitled Tag at the same moment can both pick the same
name, and the loser gets `AlreadyExists`. Found during T08 of the API review;
confirmed by reading, not reproduced.

**Fix:** Retry with the next free name on a unique violation, or pick the name inside
the insert transaction.

**Deferred:** Calcifer is a single-user local app, so this is very unlikely to happen. Revisit if it's ever seen.

**Done when:** concurrent untitled Tag creates both succeed with distinct names.

---

### I-49 · Two writes to one entity can publish their events out of commit order · low · confirmed

**Where:** `server/src/services/entity.rs` (`rename`, `set_property`), `server/src/services/richtext.rs` (`put`), `server/src/watch.rs`

**Problem:** Each write reloads its entity after commit and publishes it afterwards. The
hub's revision follows publish order, not commit order. If write A reloads, then write B
commits, reloads and publishes, and only then A publishes, the last `upserted` event
carries A's older payload. A Watch replica (T15) then shows the entity without B's
change until its next event or snapshot. The frontend used to refetch on every event,
which hid this. Related, harmless: after a lag resync, events the new snapshot covers
stay in the channel until the stream skips them, so a burst that keeps outpacing the
reader can resync more than once. Found during T14 of the API review; confirmed by
reading, not reproduced.

The same shape exists in the browser since T15: a mutation writes its response into the
replica with `writeEntity`, so a response that arrives after a newer Watch event for the
same entity overwrites it. Also, the snapshot's rich-text refetch (`model/sync.ts`) uses
`invalidateQueries`, not `writeRichTextIfNewer`, so it can race an in-flight `Put` and
cache the older doc (the open editor ignores it; a later remount starts from it and
conflict-reloads).

**Fix:** Take the revision inside the write transaction (or publish under the write
lock), so publish order matches commit order. In the browser, have `writeEntity` skip an
entity older than the cached one (by `updated_at`), and route the snapshot refetch
through `writeRichTextIfNewer`.

**Deferred:** Calcifer is a single-user local app, so this is very unlikely to happen. Revisit if it's ever seen.

**Done when:** a test with two interleaved writes to one entity sees the later commit
published last.

---

### I-61 · MCP link syntax isn't escaped for names containing `|`, `]]` or a structure prefix · low · confirmed

**Where:** `mcp-server/src/tools.ts` (`linkNotes`), `mcp-server/src/markdown/serialize.ts` (`mentionToMd`)

**Problem:** `link_notes` writes `[[${to}]]` raw, and `get_note` writes a Note's name
unescaped. A name containing `|` or `]]` is misparsed, and a name like `Todo/X` passed
to `link_notes` targets a Todo (the serializer already writes such a Note as
`[[Note/Todo/X]]`). Found in T28.

**Fix:** Define an escape for `|` and `]]` in names and use it in both writers and the
parser; have `link_notes` go through the serializer instead of formatting by hand.

**Deferred:** by the user, 2026-09-29. It needs an escape syntax for names in links first
(the proposal: a backslash escapes the next character inside `[[…]]`).

**Done when:** Notes named `A|B`, `x]]y` and `Todo/X` round-trip through `link_notes`,
`get_note` and `create_note`.

---

## Resolved

- **I-40 · Entity write transactions that read first fail with `Internal` under a race.** Fixed 2026-09-29. Every server write transaction opens through `db::begin_write` (`BEGIN IMMEDIATE`): `rename_entity`, `set_entity_property`, `delete_entity`, `persist_new_entity`, `PutRichText` and the embed worker's chunk replace. A contending writer now waits on `busy_timeout` instead of failing at once. Covered by `racing_set_property_writes_both_succeed` (it failed with `Internal: database is locked` before the fix; passed 20 repeated runs after). Observed 2026-09-29: `test:tools` passed on the first run against a fresh scratch server, with no embed worker warnings; before the fix the same first run had failed.
- **I-64 · pnpm ignores `@bufbuild/buf`'s build script.** Fixed 2026-09-29. `calcifer/pnpm-workspace.yaml` and `mcp-server/pnpm-workspace.yaml` list `@bufbuild/buf` and `esbuild` (and `msw` in `calcifer`) under `ignoredBuiltDependencies`: their scripts only link a binary the optional platform package already provides. A clean `pnpm install` in each package prints no warning, and `pnpm proto:gen` leaves the stubs unchanged. An existing `node_modules` keeps replaying the old warning (even with `--force`) until it's removed and reinstalled.
- **I-63 · ADR 2 says the TS consumers pin different `@bufbuild/protobuf` versions.** Fixed 2026-09-29. The Consequences bullet now says both consumers pin the same exact versions and generate with a local plugin; only the separate stubs remain.
- **I-62 · Relation writes look up each target's structure type twice.** Fixed 2026-09-29. `replace_scoped_links` takes `LinkTarget { id, structure_type }` and does no lookup. `check_relation_targets` returns the type it read to validate; `PutRichText` gets its targets from `live_targets`, which reads each mention's type and drops a gone target, as before.
- **I-60 · `parse.ts` contains a literal NUL byte.** Fixed 2026-09-28. `refKey`'s separator is written as the escape `\u0000` (same runtime value), so git diffs `parse.ts` as text.
- **I-55 · `proto:gen` depends on the remote buf plugin and fails under rate limits.** Fixed 2026-09-28. Both `buf.gen.yaml`s use `local: protoc-gen-es`, and `calcifer` and `mcp-server` pin `@bufbuild/protoc-gen-es` 2.15.0, `@bufbuild/buf` 1.68.2 and `@bufbuild/protobuf` 2.15.0 (the plugin's exact peer) as exact versions; `mcp-server` no longer needs a global `buf`. Regenerating left `calcifer/gen/ts/` byte-identical.
- **I-52 · Date strings aren't format-checked.** Fixed 2026-09-28. The server rejects a date value that isn't a real ISO day with `INVALID_ARGUMENT` in `CreateEntity`, `ResolveEntity` and `SetEntityProperty`, and skips a `dateChip` with a bad `date` (T24; `date_values_must_be_iso_days`, `date_chips_with_a_non_iso_date_are_skipped`). The MCP parser makes a `dateChip` only for a real day with no digit on either side (T28). The proto comments on `PropertyValue.date`, `RichText.doc` and `LinkRef.target` now say so (T29). Existing stored values aren't checked or cleaned up.
- **I-59 · Structure descriptions tell the agent to reference a Todo with `[[Name]]`.** Fixed 2026-09-28. The Todo description says `[[Todo/Name]]`, the syntax the MCP parser reads for a Todo; Note keeps `[[Name]]`.
- **I-54 · Search text drops chips and splits words across marks.** Fixed 2026-09-28. One rule (`links::collect_text`) feeds FTS and embedding chunks: inline nodes join with no separator, a mention or hashtag gives its `label`, a date chip its `date`, a `hardBreak` a space, and blocks are separated by one space. Existing documents aren't reindexed (D12); each updates on its next save. Covered by `plain_text_joins_inline_nodes_and_includes_chip_text`, `chunk_text_joins_inline_nodes_and_includes_chip_text` and `lexical_search_finds_split_words_and_mentioned_names`.
- **I-51 · Content mention links trust the doc's `structureType`.** Fixed 2026-09-28. A `mention`/`hashtag` needs only a string `id`; `replace_scoped_links` records the target's real `structure_type`, read in the query that drops a dead target. Covered by `content_links_record_the_targets_real_type`.
- **I-58 · `get_note` → write-back moves non-Note mentions onto a Note.** Fixed 2026-09-28. `get_note` writes a non-Note mention as `[[Structure/Name]]` (and a Note whose name starts with a registry prefix as `[[Note/…]]`); the parser retargets `[[X/Name]]` when `X` is exactly a registry type. The resolver creates a missing target only for a `creatable` structure; a missing DailyNote stays plain text. Observed 2026-09-28 in `test:tools` on a fresh scratch server: a note mentioning a Note, an alias, a Tag, a Todo and a DailyNote round-trips through `get_note` and `create_note` with the same links and no new entity.
- **I-53 · `get_note` renders stale mention labels and loses `[[name|label]]` targets.** Fixed 2026-09-28. `get_note` fetches each chip's target (parallel `GetEntity`, one per id) and writes it by its current name; an alias is written as the target's name, and a target that's gone is written as its label in plain text. Covered in `verify.ts` and `test:tools` (`get_note shows a renamed target by its new name`).
- **I-47 · The MCP server hard-codes the `content` rich-text property.** Fixed 2026-09-28. `tools.ts` reads `ListStructures` once per process (a failed fetch isn't cached) and addresses a body by the structure's first declared rich-text property (`docRef`); backlink labels compare against the declared ids. No `'content'` doc address is left in `mcp-server/src`.
- **I-44 · `test:tools` semantic assertion fails on a cold embedding model.** Fixed 2026-09-28. `test:tools` polls semantic search for up to 60 s (120 × 500 ms) before asserting. Observed 2026-09-28: passed on the first run against a freshly started scratch server.
- **I-14 · Deleting an entity leaves dead refs in other entities' relation values.** Fixed 2026-09-28. `DeleteEntity` reads the inbound `links` rows before sweeping them and, in the same transaction, removes the deleted id from those entities' `relation`/`relations` values; a `relation`, or a `relations` left empty, is deleted. It bumps their `updated_at` and, after commit, publishes `deleted_id` then one `upserted` per changed entity in id order (D10; no frontend change). Rich-text mentions are left alone. Dead refs stored before this change aren't cleaned up (D11): the live DB's one (Todo "Ship" → deleted Tag) needs the one-off `UPDATE` in the API review plan or a fresh DB. Covered by `delete_strips_the_id_from_relations_values` and four more tests. Observed 2026-09-28 on a scratch DB: deleting a Tag in one tab dropped it from an open Todo's tags in another tab, live.
- **I-57 · Relation pickers: any-structure `relations` renders nothing; self is offered.** Fixed 2026-09-28. The entity page no longer skips a `relations` property with an empty `target_structure`, so it renders `EntityRelationsField`, whose picker lists every structure and offers no "Create". `EntityRelationField` and `EntityRelationsField` take `selfId`, which is always in the picker's `excludeIds`. No structure declares an any-structure or self-typed relation, so this was checked by reading, not in the browser.
- **I-56 · Create and resolve callbacks are never stable.** Fixed 2026-09-28. `useCreateEntity` and `useResolveDailyNote` depend on the destructured `mutateAsync`, which is stable, not on the mutation object, which isn't. `dailyNoteDate` reads through `propertyValueOf`.
- **I-50 · `PutRichText` with an empty `entity_id` answers `NOT_FOUND`.** Fixed 2026-09-28. `GetRichText` and `PutRichText` return `INVALID_ARGUMENT` for an empty `entity_id` or `property_id` before any lookup (`require_ids`). Covered by `rich_text_with_an_empty_id_is_invalid_argument`.
- **I-48 · Entity ordering relies on the query plan in two places.** Fixed 2026-09-28. `load_entity` orders links by `link_id` and dates by `iso_date`, as `load_entities` does, and every `ListEntities` query orders by `updated_at DESC, id`. Documented in `data-model.md`. Covered by `list_ties_are_broken_by_id` and the tightened `load_entities_matches_load_entity`.
- **I-46 · `Resolve` by name can create an undated DailyNote, getting around `creatable`.** Fixed 2026-09-28. `ResolveEntity` by name with `create_if_missing` returns `FAILED_PRECONDITION` for a structure that isn't `creatable`; for DailyNote the message points to resolving by date. A lookup without create is unchanged. Covered by `resolve_by_name_of_a_missing_daily_note_is_failed_precondition`.
- **I-43 · Creating a mention or tag in the editor sends `Resolve` twice.** Fixed 2026-09-28. The root cause was never confirmed. The suggestion popup stays open while a create's `ResolveEntity` is pending, so a second Enter or click (or cmdk's own Enter on its focused root) could run `command` again. `makeSuggestion` now keeps a per-suggestion in-flight flag and ignores `command` until the create settles. Observed 2026-09-28 on a scratch DB: `#tag` by Enter and `@Note/Name` by click each sent one `ResolveEntity` and inserted one chip.
- **I-41 · `cargo fmt --check` fails on committed server code.** Fixed 2026-09-28. `cargo fmt` applied in its own commit (`7526517`), and the stale `links.rs` module comment rewritten. `cargo fmt --check` is on the API review checklist from T24.
- **I-26 · Unused property kinds, and structure flags only the frontend enforces.** Fixed 2026-09-26. The server refuses `CreateEntity` of a non-creatable structure and `RenameEntity` of a structure whose name isn't editable, both with `FAILED_PRECONDITION` (`create_of_a_daily_note_is_failed_precondition`, `rename_of_a_daily_note_is_failed_precondition`). The `text`, `number` and `relation` kinds stay and the entity page renders them (D6, variant "render"): text and number get an inline input that writes on blur or Enter (empty clears), and a single relation gets a picker (`EntityRelationField`), sharing `EntityPicker` with `EntityRelationsField`; the pickers list any structure when `target_structure` is empty. `usePropertyWriters` gains `setText`, `setNumber` and `setRelation`. On the server, `validate_property` names kinds in one exhaustive match on the declared kind, so `kind_name`'s unreachable `Richtext` arm is gone. No structure declares these kinds yet. Not yet observed in the browser.
- **I-33 · Duplicated code in the model layer.** Fixed 2026-09-26. T15 removed the duplicate `List` query functions, the raw `['entities']` keys and the Watch consumer in `App.tsx` (the list comes only from `entitiesQuery` in `model/sync.ts`). The three hand-written Timestamp-to-milliseconds conversions are gone: `timestampMsOrZero` in `model/dates.ts` wraps `timestampMs` from `@bufbuild/protobuf/wkt` (an unset Timestamp reads as 0) and backs `backlinks.ts`, `listByStructure`, `entityUpdatedAtDate` and the to-do created/updated sorts; `timestampMs` rounds to whole milliseconds, where the old code kept fractions. Delete has one path, `deleteEntity(id)` in `model/store.ts` (fire-and-forget, logs a failure); `useDeleteEntity` and `deleteEntityImperative` are gone. Create has one path per RPC: `useCreateEntity` for `CreateEntity`, and the private `resolveOrCreateEntity` for get-or-create through `ResolveEntity`, which `useResolveDailyNote` and `getOrCreateEntityForMention` both call. `useSetTodoStatus` writes through `usePropertyWriters().setSelect`, and property reads in `todos.ts` and the entity page go through the exported `propertyValueOf` (an `Object.hasOwn` check). Not yet observed in the browser.
- **I-32 · Proto and Connect details leak into components.** Fixed 2026-09-26. Components no longer build proto messages or check `ConnectError` codes. `usePropertyWriters()` in `model/store.ts` wraps `useSetProperty` with `setDate(entity, id, iso | null)`, `setSelect(entity, id, key)` and `setRelations(entity, id, targets)` (an empty list clears the property); `EntityRelationsField` emits plain `{ id, structureType }` targets. `DailyNoteDateField` detects a collision with the new `isAlreadyExists` in `model/api.ts`, and the unused `isNotFound` is gone. `withProperty` is module-private, and property reads in `store.ts` go through an `Object.hasOwn` check. Nothing under `components/`, `routes/` or `layouts/` imports `@bufbuild/protobuf` or `@connectrpc`. Not yet observed in the browser.
- **I-31 · The API contract is undocumented.** Fixed 2026-09-26. Proto comments now say that `PropertyValue.date` and `referenced_dates` are ISO `yyyy-MM-dd` days, that `RichTextRef`, `LinkRef`, `RichText.updated_at` and `Entity.referenced_dates` are output-only, that a relation value's `EntityRef` needs only `id` (a non-empty `structure_type` must match the target), and that `SearchHit.snippet` loses U+E000/U+E001. `RichText.doc` and `PutRichTextRequest.doc` point to the new [`docs/reference/richtext-doc.md`](docs/reference/richtext-doc.md), which specifies the nodes the server reads (`mention`/`hashtag` `id` and `structureType`, `dateChip` `date`, every `text`), what it derives from each (links, `referenced_dates`, FTS text, embedding chunks) and what it ignores, with an example and what each client writes. Linked from `data-model.md` and the README; `docs/specs/mentions.md` names `ResolveEntity` instead of `ResolveByName`. Comments and docs only.
- **I-13 · `buf lint` reports RPC naming errors in `services.proto`.** Fixed 2026-09-26. Adopted buf's standard naming (D7, variant A): every RPC is a verb and noun (`GetEntity`, `ListEntities`, `CreateEntity`, `RenameEntity`, `SetEntityProperty`, `DeleteEntity`, `WatchEntities`, `ResolveEntity`, `GetRichText`, `PutRichText`, `ListStructures`), takes `<Rpc>Request` and returns its own `<Rpc>Response`, which wraps the entity or doc (`GetEntityResponse { Entity entity = 1; }`, `PutRichTextResponse { RichText rich_text = 1; }`, `DeleteEntityResponse {}`). `EntityEvent` is `WatchEntitiesResponse` (same fields and numbers), `SetPropertyRequest` is `SetEntityPropertyRequest`, and `GetRichTextRequest` / `PutRichTextRequest` carry `entity_id` and `property_id` flat; `expected_updated_at` moved from `RichText` (field 4 reserved) to `PutRichTextRequest`. `RichTextRef`, `EntityRef` and `EntityRefList` stay, still used by `RichText.ref` and relations. Server, browser and MCP updated mechanically; `buf.yaml` stays on the default rules and `pnpm proto:lint` exits 0. Nothing runs it automatically: the README's proto dev loop now lists it first.
- **I-30 · `ListBacklinks` takes the wrong request and returns too little.** Fixed 2026-09-26. `ListBacklinks` takes `ListBacklinksRequest { entity_id }` and returns one `Backlink { source, source_property_id, created_at }` per link row, newest `created_at` first (then source id, then link id), and is documented as agent-only. Self-links are excluded on the server as in the frontend's `selectBacklinks`; an unknown `entity_id` is `NOT_FOUND`. Sources are hydrated in one batch (`load_entities_by_id`) instead of `load_entity` per source. The MCP server folds rows into one entry per source and `get_backlinks` names the linking properties when any isn't `content`. Covered by `list_backlinks_excludes_self_links`, `list_backlinks_reports_property_and_time_newest_first` and `list_backlinks_of_missing_entity_is_not_found`. Observed 2026-09-26 against a scratch DB through the MCP server's `getBacklinks`: a body link lists plainly, an ad-hoc `related` relation lists as `(via related)`, newest first, and a note mentioning itself doesn't list itself.
- **I-29 · Search has two RPCs for one job.** Fixed 2026-09-26. `Retrieve` and `RetrieveRequest` are gone; `Search` takes a `SearchMode` (lexical, semantic, hybrid; unspecified means hybrid, the MCP tool's default), and semantic or hybrid with embeddings off behave as lexical. Snippets are plain text: FTS5 marks matches with private-use code points, which the server strips into `SearchHit.matches`, half-open UTF-16 ranges; a vector-only hit has no snippet and no matches. `search_notes` makes one `Search` call and bolds matches with `**`. Hits are hydrated in one batch (`load_entities_by_id`, four queries) instead of `load_entity` per hit. Covered by `search_dispatches_on_mode`, `semantic_and_hybrid_fall_back_to_lexical_without_embeddings`, `match_ranges_count_utf16_units_after_non_ascii_text`, `snippets_carry_no_bracket_markers` and `hits_hydrate_as_load_entity_in_rank_order_and_skip_stale_rows`. Observed 2026-09-26 against a scratch DB through the MCP server's `searchNotes`: lexical and hybrid snippets are plain text with `**bold**` matches, and semantic mode ranks the right note first for a paraphrase.
- **I-28 · `repeated Property` should be `map<string, PropertyValue>`.** Fixed 2026-09-26. `Entity.properties` and `CreateEntityRequest.properties` are `map<string, PropertyValue>` (same field number, wire-compatible) and the `Property` message is gone, so an entity can't carry two values for one id. The server reads and writes the map (prost `HashMap`); nothing depended on property order, and `sync_relation_links` visits ad-hoc relation properties in id order. A `Create` map entry whose value has no case is `INVALID_ARGUMENT` ("property missing value"), as an entry with no value was before (`create_with_an_empty_property_value_is_invalid_argument`). The browser reads `entity.properties[id]` (the MCP server reads no entity properties); `withProperty` copies the map before setting or deleting a key. No consumer scans an entity's properties any more.
- **I-27 · Watch isn't built for keeping a full copy in sync.** Fixed 2026-09-26. Watch opens every stream with a snapshot, numbers events with a revision and sends a fresh snapshot to a subscriber that lags (T14, ADR 9). The browser's entity list is a replica fed only by Watch (`model/sync.ts`): `entitiesQuery` resolves with the first snapshot and never calls `List`; later snapshots replace the list, reset cached entities and refetch open rich-text docs; upserts and deletes are applied in place; a reconnect resyncs from its snapshot. Mutations write the server's response through the same `writeEntity` / `removeEntity`, and nothing invalidates the list, so an edit no longer reloads it. `useEntity` seeds from the replica and only calls `Get` before the first snapshot. The Vite dev proxy now ends a proxied stream whose upstream dies, so a tab notices a server restart and reconnects; any other proxy put in front of the server needs the same behaviour. Observed 2026-09-26 on a scratch DB: no `EntityService/List` after startup, typing sends only `Put`, a second tab sees creates and status changes live, and after a server restart the tab reconnected and showed an edit made while the server was down. Publish order can still differ from commit order (I-49).
- **I-38 · Stale comment about which writes publish events.** Fixed 2026-09-26. The `watch.rs` module comment now lists every publishing write, `RichText.Put` included; rewritten with the Watch snapshot and revisions (T14).
- **I-22 · A rich-text ref stored as a property value adds nothing.** Fixed 2026-09-26. `PropertyValue.richtext` is gone (field 6 and the name reserved); a document is addressed by (entity id, declared rich-text property id). `new_entity` stores no value for rich-text properties, and `Create` and `SetProperty` reject any value on one with `INVALID_ARGUMENT` (superseding I-16's `richtext`-ref case). Migration `20260926000000_drop_richtext_property_values` deletes the stored `content` rows of Note, DailyNote and Todo, chosen by the registry, and nothing else (D5). The browser builds each ref with `richTextRef(entity.id, propertyId)` from the registry's declared rich-text properties (`richTextPropertyIds`), so an editor renders for every declared one; the MCP server already addressed `content` directly. Covered by `rich_text_properties_take_no_value`, `create_by_intent_mints_id_defaults_and_name`, `drop_richtext_migration_deletes_exactly_the_richtext_rows` and `drop_richtext_migration_lists_every_declared_richtext_property`. Observed 2026-09-26 on a scratch DB written before T12: the migration removed its 7 pointer rows and kept every other property and both docs, and the old Note and daily note opened with their content. Not yet run against the live DB (27 rows to delete).
- **I-16 · Property values aren't checked against their declared kind (except select).** Fixed 2026-09-26. `validate_property` (replacing `validate_select`) requires every declared property's value case to match its `PropertyKind`, in `Create`, `Resolve` and `SetProperty`; a rich-text property takes only a `richtext` ref. Undeclared property ids still take any value except `select`. Covered by `values_must_match_the_declared_kind`.
- **I-24 · Looking up an entity by name isn't reliable for most structures.** Fixed 2026-09-26. `find_by_name` orders by `created_at, id`, so of several same-named entities the oldest wins and the lowest id breaks ties (D2; documented on `Resolve`). `Create`, `Resolve` by name and a non-empty `List` filter return `INVALID_ARGUMENT` for a structure type not in the registry. Covered by `resolve_by_name_with_duplicates_returns_the_oldest` and `unknown_structure_types_are_rejected`.
- **I-25 · Wrong error message on a unique-name clash.** Fixed 2026-09-26. `map_unique_violation` branches on the column SQLite reports (`entities.date_key` or `entities.name`) and returns "a DailyNote for <date> already exists" or `a Tag named "<name>" already exists`, for `Create`, `Rename`, `Resolve` and `SetProperty(date)`. Covered by `create_onto_a_taken_tag_name_is_already_exists`, `rename_onto_a_taken_tag_name_is_already_exists` and `set_property_date_onto_existing_day_is_already_exists`.
- **I-20 · `Entity` is both the write input and the read output.** Fixed 2026-09-26. `Entity` is output-only: `Create` takes `{ structure_type, optional name, properties }` and the server builds the entity with `new_entity` (id, timestamps, defaults), so client links and referenced dates can't reach it; `Update` and `UpdateEntityRequest` are gone. Covered by `create_by_intent_mints_id_defaults_and_name`; the browser and MCP scripts use the id the server returns. Observed 2026-09-26 against a scratch DB: "+ New" makes Notes, Tags and To-dos with server-minted ids, and MCP `create_note` still derives links.
- **I-21 · Default entities are built in four places.** Fixed 2026-09-26. `new_entity` is the only builder and now owns the default name (`Untitled <structure name>`, with the first free `Untitled Tag N` for Tags); the frontend's `buildEntityMessage` and `defaultNameFor` are gone, and the entity page focuses the title from a `justCreated` history state set by "+ New" instead of comparing names. Covered by `create_second_untitled_tag_gets_a_free_name`. Observed in the browser 2026-09-26: each "+ New" lands on a focused `Untitled <name>` title, a second Tag gets `Untitled Tag 2`, and opening an existing entity doesn't focus the title.
- **I-15 · Renames and daily-note moves still resend every property.** Fixed 2026-09-26. New `EntityService.Rename` writes only the name, its FTS row and `updated_at`; the title input's debounced rename uses `useRenameEntity` (optimistic on `name`, rolled back per entity). Covered by `rename_concurrent_with_set_property_keeps_both`, `rename_changes_only_the_name` and `rename_missing_id_is_not_found`. Observed 2026-09-26: a browser rename and a second client's `SetProperty(status)`, in both orders, both survive a reload.
- **I-23 · Daily notes are created and moved differently by each client.** Fixed 2026-09-25. `EntityService.Resolve` (a `name` or `date` key, `create_if_missing`, returns `created`) replaces `ResolveByName` and `CreateDailyNote`; the browser and the MCP server both get-or-create daily notes through it, and the MCP list-and-scan fallback is gone. The server names a DailyNote for its date on Create, Update, SetProperty and Resolve, so the browser moves a note with a plain `SetProperty(date)` and shows its inline error on `ALREADY_EXISTS`. Covered by `set_property_date_renames_daily_note`, `resolve_by_date_gets_or_creates` and nine more tests. Not yet observed in the browser. `name_editable` is still not enforced (I-26).
- **I-37 · A racing `RichText.Put` fails with `Internal`, not `FailedPrecondition`.** Fixed 2026-09-25. `Put` opens its transaction with `BEGIN IMMEDIATE`, so a racing `Put` waits on `busy_timeout` and then fails the expectation check. A file-backed race test fails without the fix (checked 5 of 5 runs) and passes with it; three simultaneous MCP appends to one note all landed.
- **I-39 · Leaving a new daily note within the save debounce deletes what was typed.** Fixed 2026-09-25. The editor sends a pending debounced save when it unmounts, and the prune waits for that doc's outstanding saves before checking the server. Observed in the browser against a scratch DB: typing and leaving the day at once kept the note and its text; an empty note was still pruned.
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
