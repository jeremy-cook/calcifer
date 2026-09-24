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

### I-1 · Title input can revert mid-typing · high · confirmed

**Where:** `calcifer/src/App.tsx` (`useWatchSync`), `calcifer/src/routes/e.$id.tsx` (`EntityTitleInput`)

**Problem:** Renaming sends one `Update` per keystroke. The watch-stream handler writes
every `upserted` event straight into `qk.entity(id)`. If the server echo of keystroke *n*
arrives after keystroke *n+2* was applied optimistically, the cache goes back to the
older name. `EntityTitleInput`'s effect then syncs `entity.name` into local state, so the
field visibly reverts to a shorter prefix. The component's comment claims this can't
happen; it only considers the optimistic path, not watch echoes.

**Fix:**
- Debounce the rename (~300 ms) so a typing burst sends one write.
- Don't sync `entity.name` into the input while it has focus.
- Optionally, in `useWatchSync`, skip an upsert whose `updatedAt` is older than the
  cached entity's.

**Done when:** typing quickly into a title with added network latency never reverts
characters, and a rename from another client still shows up when the field isn't
focused.

---

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

### I-3 · `Update` can change an entity's `structure_type` · medium · confirmed

**Where:** `server/src/services/entity.rs` (`update`)

**Problem:** The `UPDATE entities SET structure_type = ?, …` statement writes the
client's value, so any `Update` can turn a Note into a Tag. Nothing in the app needs to
change an entity's type, and doing so breaks property and link assumptions (e.g. a
DailyNote without a `date`).

**Fix:** Drop `structure_type` from the `UPDATE`. Also read the stored type first and
return `InvalidArgument` if the request's differs, so callers learn about it rather than
having it silently ignored.

**Done when:** an `Update` with a different `structure_type` is rejected, and the stored
type is unchanged.

---

### I-4 · To-do "today" doesn't roll over at midnight · medium · confirmed

**Where:** `calcifer/src/model/todos.ts` (`useTodos`)

**Problem:** `todayIso()` runs inside a `useMemo` whose deps are
`[entities, filter, sort]`. If the app stays open past midnight, the Overdue, Today and
This week filters use yesterday's date until the entity list changes. `TodoRow`'s
overdue styling reads `todayIso()` on every render, so it can disagree with the filter.

**Fix:** Add a `useToday()` hook in `~/model/dates` that returns `todayIso()` and
re-renders at the next local midnight (a `setTimeout` to midnight, rescheduled each time
it fires). Pass its value into `useMemo` as a dependency, and use it in `TodoRow` and
anywhere else that calls `todayIso()` during render.

**Done when:** with the system clock moved past midnight, the list and the row styling
update without a reload.

---

### I-5 · A daily-note move the server rejects fails silently · low · confirmed

**Where:** `calcifer/src/components/calendar/DailyNoteDateField.tsx`, `calcifer/src/model/store.ts` (`useUpdateEntity`)

**Problem:** `validate` checks whether the target day is taken using the client's cached
list. If the cache is stale, the server's `date_key` unique index rejects the update.
`useUpdateEntity` rolls back and the date snaps back with no message. This predates the
`useUpdateEntity` refactor.

**Fix:** Let `useUpdateEntity` take an `onError` callback, or return `mutateAsync`, so
callers can react to failures. `DailyNoteDateField` should show the server's "already
exists" error. A shared toast for failed writes would cover every caller.

**Done when:** a move that collides on the server shows an error instead of silently
reverting.

---

### I-6 · Concurrent optimistic updates can undo each other on rollback · low · confirmed

**Where:** `calcifer/src/model/store.ts` (`useUpdateEntity`)

**Problem:** Each mutation snapshots the whole list in `onMutate` and restores that
snapshot in `onError`. If A and B are both in flight and A fails, A's snapshot (taken
before B) overwrites B's optimistic change until the `onSettled` refetch lands. It
corrects itself, but causes a visible flicker. The hooks it replaced had the same flaw.

**Fix:** On error, restore only the failed entity: put `prevEntity` back into the list
with `map`, rather than restoring the whole list snapshot.

**Done when:** a failed update to one to-do doesn't flicker a concurrent change to
another.

---

### I-7 · `TodoRow` subscribes to the whole entity list · low · confirmed

**Where:** `calcifer/src/components/todo/TodoRow.tsx`, `TodoList.tsx`

**Problem:** Every row calls `useAllEntities()` and resolves each tag with a linear
`find`, which is O(rows × tags × entities). Every list change re-renders every row. This
is fine at current data sizes.

**Fix:** In `TodoList`, build one `Map<id, Entity>` (memoized on the entity list) and
pass the resolved tags into each row. Wrap `TodoRow` in `memo`.

**Done when:** `TodoRow` no longer calls `useAllEntities()`.

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

### I-10 · The structure registry is defined three times · medium · design debt

**Where:** `calcifer/src/model/structures.ts`, `server/src/structures.rs`, `mcp-server/src/tools.ts`

**Problem:** Structure types, property ids, select options/defaults and relation
properties are hand-copied into the frontend, the Rust server and the MCP server. Every
new structure or property means three edits that can drift. I-2 and I-9 add more to the
Rust copy. Raised in the original review.

**Fix:** Make the server the source of truth and expose it over a `ListStructures` RPC.
Could be defined in proto or in a Rust table; `list_structures` already exists as an MCP
tool. The frontend keeps only presentation data (icons, colors) keyed by type and fetches
the rest. The MCP server reads the RPC. Worth an ADR, since it changes where schema lives.

**Done when:** adding a select option is a one-file change.

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

### I-12 · Eight ESLint errors on `main` · low · confirmed

**Where:** `calcifer/src/components/ui/badge.tsx`, `ui/button.tsx`,
`editors/tiptap/components/date/showDatePicker.ts`, `routes/e.$id.tsx`

**Problem:**

| File | Errors | Rule |
|---|---|---|
| `badge.tsx`, `button.tsx` | 1 each | `react-refresh/only-export-components`: shadcn `*Variants` exports |
| `showDatePicker.ts` | 1 | `prefer-const` |
| `e.$id.tsx` | 4 | `react-refresh/only-export-components`: sub-components in a route file |
| `e.$id.tsx` | 1 | `react-hooks/set-state-in-effect`: title sync effect, see I-1 |

**Fix:**
- `prefer-const`: auto-fixable.
- Route files: move sub-components out to `components/entity/`, or turn off
  `only-export-components` for `src/routes/**`, since TanStack route files always mix
  exports.
- shadcn exports: turn the rule off for `components/ui/**`.
- `set-state-in-effect`: goes away with the I-1 fix.

**Done when:** `pnpm lint` exits 0.

---

## Resolved

- **I-8 · Updating a missing entity returns a foreign-key error.** Fixed 2026-09-24. Update checks \`rows_affected()\` and returns \`NotFound\` for an unknown id; covered by \`update_missing_entity_is_not_found\`.

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
