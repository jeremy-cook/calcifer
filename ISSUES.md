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

> **RPC names.** Entries written before 2026-09-26 use the RPC names from before I-13's
> buf rename. Read `Get`/`List`/`Create`/`Rename`/`SetProperty`/`Delete`/`Watch`/`Resolve`
> as `GetEntity`/`ListEntities`/`CreateEntity`/`RenameEntity`/`SetEntityProperty`/
> `DeleteEntity`/`WatchEntities`/`ResolveEntity`, and `RichText.Get`/`Put` as
> `GetRichText`/`PutRichText`. Server handlers follow the same names in snake_case
> (e.g. `delete_entity`). `ResolveByName` was replaced by `ResolveEntity` in T07.

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

### I-26 · Unused property kinds, and structure flags only the frontend enforces · low · confirmed

**Where:** `server/src/structures.rs` (registry), `calcifer/src/routes/e.$id.tsx:257`

**Problem:** No structure declares a `relation`, `text` or `number` property, and the
entity page silently renders nothing for them (`default: return null`). The server
doesn't enforce `creatable` or `name_editable`; only the frontend does.

**Fix:** Either remove the unused kinds or render them. Enforce `creatable` and
`name_editable` on the server once I-20 and I-23 have landed.

**Done when:** every `PropertyKind` is either used and rendered or removed, and the server
rejects creating a non-creatable structure and renaming a non-name-editable entity.

**Progress:** The flag half is done (2026-09-26): `Create` of a non-creatable structure and
`Rename` of a structure with `name_editable: false` return `FAILED_PRECONDITION`, covered by
`create_of_a_daily_note_is_failed_precondition` and
`rename_of_a_daily_note_is_failed_precondition`. The unused-kinds half remains.

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

**Progress:** T15 removed the `List` query functions (the list now comes only from
Watch, through `entitiesQuery` in `model/sync.ts`), the raw `['entities']` keys, and
the Watch consumer in `App.tsx`. Left for T22: the three timestamp conversions and the
duplicate delete and create paths.

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

### I-40 · Entity write transactions that read first fail with `Internal` under a race · medium · confirmed

**Where:** `server/src/services/entity.rs` (`set_property`), `server/src/embed/worker.rs:133`

**Problem:** Same class as I-37. These transactions start DEFERRED, read (the
structure type, the previous property value, the old chunk ids), then write. If another
connection commits in between, the upgrade to a writer fails with `SQLITE_BUSY` right
away, ignoring `busy_timeout`. The caller gets `Internal`, or the embed worker drops a
batch. Found during T06a of the API review; confirmed by reading, not reproduced.

**Fix:** Open them with `begin_with("BEGIN IMMEDIATE")`, as `RichText.Put` does since
I-37.

**Deferred:** Calcifer is a single-user local app, so this is very unlikely to happen. Revisit if it's ever seen.

**Done when:** each one takes the write lock before its first read, and a race test
covers `set_property`.

---

### I-41 · `cargo fmt --check` fails on committed server code · low · confirmed

**Where:** `server/src/services/entity.rs`, `search.rs`, `link_store.rs`, `embed/chunk.rs`, parts of `services/richtext.rs`

**Problem:** Several committed files aren't rustfmt-clean, so any engineer who runs
`cargo fmt` produces unrelated churn. Nothing checks formatting today. Found during
T06a of the API review.

**Fix:** One `cargo fmt` commit with no other changes, then optionally a check.

**Done when:** `cargo fmt --check` passes.


**Also (found in T20):** the module comment at `server/src/links.rs:3-4` points at `extractDocReferences` in `calcifer/src/model/linkSync.ts`, which no longer exists. Fix it in the same pass.
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

### I-43 · Creating a mention or tag in the editor sends `Resolve` twice · low · confirmed

**Where:** `calcifer/src/editors/tiptap/components/mention/makeSuggestion.ts` (`command` → `resolveMentionItem`)

**Problem:** Picking "Create new Note: …" or "Create new Tag: …" sent two `Resolve`
calls for one entity (observed in the browser during T07's checks). The server
dedupes by name, so only one entity is made. The cause wasn't traced; the call path
predates T07.

**Fix:** Find why the suggestion command runs twice (e.g. Enter handled by both the
menu and the suggestion plugin) and send one call.

**Done when:** creating a mention or tag sends one `Resolve`.

---

### I-44 · `test:tools` semantic assertion fails on a cold embedding model · low · confirmed

**Where:** `mcp-server/src/tools-test.ts` (semantic retrieval poll, about l.57)

**Problem:** On a fresh server the embedding worker is still loading its model, and
the test's 15 s poll can run out before both notes are embedded. The first run after a
server start failed `semantic retrieval surfaces both related notes`; a second run
passed. Found during T07's integration check.

**Fix:** Poll longer, or wait for the embed queue to drain before asserting.

**Done when:** `pnpm test:tools` passes on the first run against a fresh scratch server.

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

### I-46 · `Resolve` by name can create an undated DailyNote, getting around `creatable` · low · confirmed

**Where:** `server/src/services/entity.rs` (`resolve_name`)

**Problem:** Since T09, `Create` refuses a DailyNote (`creatable = false`) and points at
`Resolve` by date. `Resolve` by name with `structure_type: "DailyNote"` and
`create_if_missing` still creates one, with no date. The browser avoids this path
(`makeSuggestion.ts`), but the server doesn't refuse it. Found during T09 of the API
review.

**Fix:** In `resolve_name`, return `FailedPrecondition` when it would create an entity
of a non-creatable structure (a lookup without create still works).

**Done when:** `Resolve { name, structure_type: "DailyNote", create_if_missing }` for a
missing name is `FailedPrecondition`, and a test covers it.

---

### I-47 · The MCP server hard-codes the `content` rich-text property · low · confirmed

**Where:** `mcp-server/src/tools.ts:23`, `mcp-server/src/markdown/verify.ts:65`

**Problem:** Since T12 a rich-text doc is addressed by (entity id, declared rich-text
property id), and the frontend reads the declared id from the structure registry. The
MCP server still writes `propertyId: 'content'` directly. It works today because every
rich-text structure declares `content`, but a structure with another rich-text property
would break it. Found during T12 of the API review.

**Fix:** Read the declared rich-text property from `StructureService` (as the frontend's
`richTextPropertyIds` does).

**Done when:** no `'content'` literal addresses a rich-text doc in `mcp-server/src`.

---

### I-48 · Entity ordering relies on the query plan in two places · low · confirmed

**Where:** `server/src/services/entity.rs` (`load_entity`, `load_entities`)

**Problem:** `load_entity` has no `ORDER BY`, so the order of an entity's properties,
links and referenced dates comes from the primary-key index the plan happens to use.
`load_entities` (T13) orders them explicitly and matches it today. Also, `List` orders
by `updated_at DESC` with no tie-breaker, so entities written in the same millisecond
come back in plan order. Found during T13 of the API review.

**Fix:** Add the same explicit `ORDER BY`s to `load_entity`, and `, id` to List's order.

**Done when:** both loaders order explicitly and a List tie is broken by id.

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

### I-50 · `PutRichText` with an empty `entity_id` answers `NOT_FOUND` · low · confirmed

**Where:** `server/src/services/richtext.rs` (`put_rich_text`, `check_declared`)

**Problem:** Since T19 the request is flat (`entity_id`, `property_id`, …), so there's no
`ref` to be missing. An empty `entity_id` now fails `check_declared` with `NOT_FOUND`
"entity " instead of the old `INVALID_ARGUMENT` "richtext missing ref". Found during
T19 of the API review.

**Fix:** Return `INVALID_ARGUMENT` for an empty `entity_id` or `property_id` in
`GetRichText` and `PutRichText`, before the lookup.

**Done when:** both RPCs answer `INVALID_ARGUMENT` for an empty id, and a test covers it.

---

### I-51 · Content mention links trust the doc's `structureType` · low · confirmed

**Where:** `server/src/link_store.rs` (`replace_scoped_links`), `server/src/links.rs:53-64`

**Problem:** A rich-text `mention` or `hashtag` records `attrs.structureType` as the link's `target.structure_type` without checking the target's real type, so a wrong attribute produces a `LinkRef` that lies (relation links are checked since I-2). A chip whose `structureType` is null is skipped silently: no link and no error. The editor's default for that attribute is `null` (`entityMention.ts:10`), so chips pasted from HTML without `data-structure-type` lose their link. Found during T20 of the API review.

**Fix:** Look up each target's type when deriving content links: use the real type, and drop (or report) a dead target. Don't depend on `structureType` being present.

**Done when:** a mention with a wrong or null `structureType` produces a link with the target's real type, and a test covers both.

---

### I-52 · Date strings aren't format-checked · low · confirmed

**Where:** `server/src/links.rs:66-76` (dateChip), `server/src/services/entity.rs` (`validate_property`, `date_key_from`), `mcp-server/src/markdown/parse.ts:66`

**Problem:** A `dateChip`'s `date` goes into `referenced_dates` unchecked, and `PropertyValue.date` isn't checked in `CreateEntity` or `SetEntityProperty` (only `ResolveEntity`'s date key is). A non-ISO DailyNote `date` becomes its `date_key`, and its name falls back to the raw string. The MCP parser's date regex accepts impossible days such as `2026-13-45`. Found during T20 of the API review.

**Fix:** Validate `yyyy-MM-dd` as a real calendar day in all three server paths (`INVALID_ARGUMENT` for a property; skip a bad chip), and tighten the MCP regex to real days.

**Done when:** a bad date is rejected by `SetEntityProperty` and `CreateEntity`, ignored in a chip, and not produced by the MCP parser, with tests.

---

### I-53 · `get_note` renders stale mention labels and loses `[[name|label]]` targets · low · confirmed

**Where:** `mcp-server/src/markdown/serialize.ts:22-25`

**Problem:** `get_note` writes each mention from its stored `label`, which nothing updates when the target is renamed, so the agent reads old names. A `[[name|label]]` link serializes back as `[[label]]`, so a read-modify-write through the agent can point it at a different note. Found during T20 of the API review.

**Fix:** Serialize mentions from the target entity's current name (the MCP server can resolve ids), and keep the alias form when the label differs.

**Done when:** a renamed target shows its new name in `get_note`, and `[[name|label]]` round-trips.

---

### I-54 · Search text drops chips and splits words across marks · low · confirmed

**Where:** `server/src/links.rs:88-112` (`extract_plain_text`), `server/src/embed/chunk.rs:89-101`

**Problem:** Plain text for FTS and embeddings leaves out chips, so a mention's label, a tag's name and a date in the body aren't searchable. It also joins adjacent text nodes with a space, so `**bold**er` is indexed as `bold er`. Documented in `docs/reference/richtext-doc.md`. Found during T20 of the API review.

**Fix:** Emit chip labels (and dates) into the plain text, and join adjacent inline text nodes without a separator (separate only between blocks).

**Done when:** a note is found by a word split across marks and by a mentioned name, with tests on `extract_plain_text`.

---

### I-55 · `proto:gen` depends on the remote buf plugin and fails under rate limits · low · confirmed

**Where:** `calcifer/buf.gen.yaml`, `mcp-server/buf.gen.yaml`

**Problem:** Both use the remote `buf.build/bufbuild/es` plugin, so `pnpm proto:gen` fails when the BSR rate-limits (T20 needed seven retries). `calcifer` has a local `protoc-gen-es`, but at v2.11.0 against the remote v2.15.0. Found during T20 of the API review.

**Fix:** Use a local `protoc-gen-es` pinned to one version in both packages.

**Done when:** `proto:gen` works offline in both packages and both produce the same generated code version.

---

## Resolved

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
