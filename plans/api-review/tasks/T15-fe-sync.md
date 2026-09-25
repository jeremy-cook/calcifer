# T15 · FE: replica from Watch; apply events; drop refetches

**Area:** fe · **Issues:** I-27 (B3), I-33 (D2, the sync-related parts)

## Background
Read ADR 9 in `docs/adr/`. `EntityService.Watch` now works like this (see the comments
in `services.proto`):
- The first message is `snapshot` (every entity).
- Then come `upserted`, `deletedId` and `richTextChanged` events with an increasing
  `revision`.
- On lag the server sends another `snapshot`, which replaces everything.
- Snapshots don't include rich-text docs.

## Current FE problems
- `calcifer/src/App.tsx` `useWatchSync`:
  - it starts `List` and `Watch` concurrently, so there's a startup race;
  - it never refetches after a reconnect;
  - it invalidates `['entities']` on every event.
- `model/store.ts`:
  - `useAllEntities` has its own `List` query function (a duplicate of App's);
  - `onEntityWritten` and several mutations call `invalidateQueries({ queryKey:
    ['entities'] })`, so every save causes a full list reload.
- `model/richtext.ts`: `usePutRichText` invalidates the entity and the list.
- The raw `['entities']` key is used instead of `qk`.

## Requirements
- **New `calcifer/src/model/sync.ts`.** It owns the Watch loop, moved out of `App.tsx`.
  `App.tsx` only calls the hook.
  - `snapshot`: replace `qk.entities()` with the entities, reset every `qk.entity(id)`
    (set the present ones, remove the absent ones), and invalidate `richtext` queries so
    open docs refetch. Keep T06's rule that an editor with a pending local save isn't
    overwritten.
  - `upserted`: replace-or-insert in the list and set `qk.entity(id)`.
  - `deletedId`: remove from the list and remove `qk.entity(id)`.
  - `richTextChanged`: move T06's handling here unchanged.
  - Reconnect: keep the retry loop. The new stream's first snapshot resyncs, and nothing
    else is needed.
- **One write path into the cache.** Export `writeEntity(entity)` and
  `removeEntity(id)` from `sync.ts` (or `store.ts`, whichever avoids an import cycle).
  The Watch handlers and every mutation's `onSuccess` use them. Optimistic updates and
  per-entity rollback (I-6) keep working.
- **`entitiesQuery`.** Export it the way `structuresQuery` is exported.
  - Its `queryFn` resolves with the first snapshot (a promise from `sync.ts`).
  - `staleTime: Infinity`; it never calls `List`.
  - `useAllEntities` and `getEntitiesSnapshot` use it.
- **Delete** every `invalidateQueries` on `['entities']`, `qk.entities()` or
  `qk.entity(...)` that exists only to pick up server changes, and replace every raw
  `['entities']` key with `qk`.
- `useEntity(id)` can keep its `Get` query for deep links before the snapshot arrives,
  but it must not refetch on every write.

## Out of scope
Timestamp helpers and duplicate create/delete paths (T22). Proto leaks (T21). Server
changes.

## Done when
- `grep -rn "invalidateQueries" calcifer/src` shows only justified uses, each with a
  one-line comment saying why.
- `grep -rn "entityClient.list" calcifer/src` finds nothing.
- `grep -rn "\['entities'\]" calcifer/src` finds nothing.
- `pnpm build` and `pnpm lint` pass.
- Browser checks to report:
  1. The network panel shows no `List` call at startup or later.
  2. Typing in a note sends only `Put` requests.
  3. A second tab shows renames, new entities, deletes and rich-text changes live.
  4. Stop and restart the server: the tab reconnects and reflects changes made while it
     was down.
- `ISSUES.md`: move I-27 to Resolved. Add a line to I-33 listing what's left for T22.

## Commits
1. `fe: build the entity replica from Watch snapshots and events (I-27)`
2. `fe: write mutation results into the replica instead of refetching (I-27, I-33)`
3. `docs: resolve I-27; note I-33 progress`
