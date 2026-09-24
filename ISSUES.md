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
