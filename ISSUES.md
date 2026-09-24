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

### I-2 · Relation targets aren't checked against `targetStructure` · medium · confirmed

**Where:** `server/src/link_store.rs` (`sync_relation_links`), `server/src/structures.rs`

**Problem:** The server stores whatever `structure_type` the client puts on each
`EntityRef` and never checks it against the property's declared target. Any writer
(including the MCP agent) can put a Note or a Todo in a Todo's `tags`. The stored
`target_structure` can also disagree with the target's real type.

**Fix:** Declare each relation property's target in `structures.rs` (e.g.
`relation_properties: &[("tags", "Tag")]`). In `sync_relation_links`, load each target's
real `structure_type` from `entities` and return `InvalidArgument` on a mismatch. Write
the real type, not the client's, to `links.target_structure`.

**Done when:** an `Update` with a Note in a Todo's `tags` is rejected, and a test covers
it.

**Related:** I-9 and I-10. All three are "the server doesn't enforce the schema".

---

### I-9 · Select values aren't checked against the allowed options · medium · confirmed

**Where:** `server/src/services/entity.rs` (`update`, `create`), `server/src/structures.rs`

**Problem:** The server accepts any string for a `select` property, e.g.
`status = "banana"`. The frontend quietly treats unknown values as the default, so the
stored value and the displayed value disagree. The MCP agent is the likeliest source of
bad values. Raised in the original review.

**Fix:** Best done after I-10, so the options aren't copied a third time. As a stopgap,
replace `select_defaults` with full option lists in `structures.rs` and reject values not
in the list.

**Done when:** an `Update` with an unknown select value is rejected.

---

### I-11 · Property edits send the whole entity (last write wins) · low now, medium once the agent edits to-dos · design debt

**Where:** `calcifer/src/model/store.ts` (`useUpdateEntity`), `EntityService.Update`

**Problem:** Toggling a checkbox sends every property of the entity. If the browser and
the MCP agent edit different properties of the same entity at the same time, one write
silently undoes the other. Raised in the original review.

**Fix:** Add `EntityService.SetProperty(entity_id, property_id, value | clear)` that
updates one property row and runs relation link sync for just that property. Move
`withProperty` callers onto it. Keep `Update` for renames and bulk edits.

**Done when:** concurrent edits to two different properties both survive.

---

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

## Resolved

- **I-8 · Updating a missing entity returns a foreign-key error.** Fixed 2026-09-24. Update checks `rows_affected()` and returns `NotFound` for an unknown id; covered by `update_missing_entity_is_not_found`.
- **I-3 · `Update` can change an entity's `structure_type`.** Fixed 2026-09-24. Update no longer writes `structure_type`; a mismatch with the stored type returns `InvalidArgument`; covered by `update_rejects_structure_type_change`.
- **I-1 · Title input can revert mid-typing.** Fixed 2026-09-24. Renames are debounced (300 ms) and flushed on blur/unmount, and `entity.name` is not synced into the input while it is focused. Verified by reasoning; not reproduced in the browser.
- **I-12 · Eight ESLint errors on `main`.** Fixed 2026-09-24. ESLint config turns off `only-export-components` for `src/routes/**` and `src/components/ui/**`; `prefer-const` fixed by hand. `pnpm lint` exits 0.
- **I-6 · Concurrent optimistic updates can undo each other on rollback.** Fixed 2026-09-24. Rollback replaces only the failed entity (list row and detail cache), never the whole list snapshot. Verified by reasoning.
- **I-5 · A daily-note move the server rejects fails silently.** Fixed 2026-09-24. `useUpdateEntity` takes an `onError`; `DailyNoteDateField` shows an inline error on `AlreadyExists`. No shared toast (no toast library). Verified by reasoning.
- **I-4 · To-do "today" doesn't roll over at midnight.** Fixed 2026-09-24. `useToday()` in `model/dates.ts` re-renders at local midnight and re-checks on focus/visibility; used by the to-do list, rows and calendar route. Midnight arithmetic exercised by script (incl. DST); clock change not observed in the browser.
- **I-7 · `TodoRow` subscribes to the whole entity list.** Fixed 2026-09-24. `TodoList` resolves tags through one memoized `Map` and passes them in; `TodoRow` is memoized and no longer calls `useAllEntities()`.
- **I-10 · The structure registry is defined three times.** Fixed 2026-09-24. Registry authored only in `server/src/structures.rs`, served by `StructureService.List` (ADR 7); frontend keeps icons/colors and fetches the rest; MCP `list_structures` reads the RPC. Verified by adding a priority option to `structures.rs` alone: server, frontend and MCP all build and test clean. Generated TS now carries `// @ts-nocheck` because proto enums aren't erasable syntax.

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
