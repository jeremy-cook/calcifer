# T10 · FE: move writes to Create/Rename/Resolve/SetProperty; delete builders

**Area:** fe · **Issues:** I-20 (A1), I-21 (A2), I-23 (A4), I-15

## Background
The server now builds entities (ADR 8, `docs/adr/`). See `proto/calcifer/v1/services.proto`:
- `Create({ structureType, name?, properties })`: the server mints the id, defaults
  and name (`Untitled <Structure>`, made unique for Tags). A DailyNote is **not**
  creatable this way.
- `Rename({ id, name })`: name only. A DailyNote can't be renamed.
- `Resolve({ structureType, key: name | date, createIfMissing })` → `{ entity, created }`.
- `SetProperty(date)` on a DailyNote also renames it and fails with `ALREADY_EXISTS` if
  that day has a note.
- `Update`, `ResolveByName`, `CreateDailyNote` and `Create({ entity })` are deprecated
  and removed in T12.

## Current FE (in `calcifer/src/`)
- `model/store.ts`: `defaultName` (l.22), `buildEntityMessage`, `buildDailyNoteMessage`,
  `useCreateEntity`, `useCreateDailyNote`, `useUpdateEntity`, `withName`,
  `withDailyNoteDate`, and the mention flow's `resolveByName` (about l.298).
- Callers:
  - `layouts/sidebar/NewButton.tsx`, `components/todo/TodoQuickAdd.tsx` (create);
  - `routes/e.$id.tsx` (title rename via `useUpdateEntity(withName)`; `isDefaultName` at
    about l.166 decides title autofocus);
  - `components/calendar/DailyNoteDateField.tsx` (move via `withDailyNoteDate`);
  - `components/calendar/DailyNoteSection.tsx` (create a day's note).

## Requirements
- **Create:** `useCreateEntity(structureType, name?)` calls Create-by-intent.
  `buildEntityMessage` and `defaultName` go.
- **Daily-note create:** `useCreateDailyNote(iso)` becomes a Resolve with the `date` key
  and `createIfMissing: true`. `buildDailyNoteMessage` goes.
- **Mentions:** the mention and wikilink resolve helper uses `Resolve` with the `name`
  key.
- **Rename:** add `useRenameEntity()`.
  - Optimistic on `name` only, in both the list cache and the entity cache.
  - Roll back only `name` on error, following the per-entity rollback pattern from I-6.
  - The title input uses it, and keeps its existing debounce/flush behaviour (I-1).
- **Daily-note move:** `DailyNoteDateField` uses the existing `useSetProperty` for
  `date`. On `ALREADY_EXISTS`, show the existing inline error. The new name comes from
  the server response. `withDailyNoteDate` goes.
- **Title autofocus:** replace the `isDefaultName` string comparison with an explicit
  "just created" signal from the create flows. Recommended: router history state, e.g.
  `navigate({ …, state: { justCreated: true } })`, read in the entity route. A search
  param is acceptable if state is awkward with TanStack Router's types.
- **Delete** `useUpdateEntity`, `withName` (if unused) and any helper left without
  callers. Nothing in `calcifer/src` calls `entityClient.update`, `resolveByName` or
  `createDailyNote`, or passes `{ entity }` to `create`.
- Follow `CLAUDE.md` component style.

## Out of scope
- How the rich-text editor finds its document (T11). Keep reading the `richtext`
  property value for now; the server still writes it.
- The `['entities']` invalidations (T15).
- Proto helpers leaking into components (T21).

## Done when
- The grep above is clean, and `pnpm build` and `pnpm lint` pass.
- Browser checks to report:
  1. "+ New" for each creatable structure lands on a focused title.
  2. Two "+ New Tag" in a row both succeed.
  3. Rename a Todo while setting its status via MCP or a second tab; both persist.
  4. Change a daily note's date; its title follows. Moving onto a day that has a note
     shows the inline error.
  5. Typing `[[New thing]]` and `#newtag` in a note still creates and links them.

## Commits
1. `fe: create entities and daily notes by intent (I-20, I-21, I-23)`
2. `fe: rename through Rename and move daily notes through SetProperty (I-15, I-23)`
3. `fe: remove client-side entity builders (I-21)`
