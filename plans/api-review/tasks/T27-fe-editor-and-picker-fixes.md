# T27 · FE: one resolve per mention create, stable callbacks, relation pickers

**Area:** fe · **Issues:** I-43, I-56, I-57

## Background
Three small frontend defects. Read each entry in `ISSUES.md`. Entries written before
2026-09-26 use old RPC names; the note at the top of "Open" maps them
(`Resolve` = `ResolveEntity`).

- **I-43:** picking "Create new Note: …" or "Create new Tag: …" in the editor's mention
  menu was seen to send two `ResolveEntity` calls. The server dedupes by name, so only
  one entity is made. The cause wasn't traced. The path:
  `editors/tiptap/components/mention/makeSuggestion.ts` (`command` →
  `resolveMentionItem` → `getOrCreateEntityForMention` in `model/store.ts`), with keys
  from `MentionMenu.tsx` and the popup in `components/suggestionPopup.ts`. `command` is
  async and nothing stops it from running again while a resolve is in flight; the
  suggestion stays active until the mention is inserted.
- **I-56:** `useCreateEntity` and `useResolveDailyNote` in `model/store.ts` list the
  whole `useMutation` result (`m`) in their `useCallback` dependencies, so the callbacks
  change on every render. `dailyNoteDate` reads `entity.properties.date` directly rather
  than through `propertyValueOf`.
- **I-57:** in `routes/e.$id.tsx`'s property rendering, the `RELATIONS` case returns
  `null` when `def.targetStructure` is empty, though an empty target means any structure
  and `EntityPicker` supports it. Neither `EntityRelationsField` nor
  `EntityRelationField` leaves the entity being edited out of the picker's candidates.

## Requirements
- **I-43:** trace why the command runs twice and fix the cause if you find it. Either
  way, make a create-item command idempotent while it's in flight: a second invocation
  for the same suggestion, while the first resolve is pending, does nothing. Report the
  cause, or say it wasn't found.
- **I-56:** depend on `m.mutateAsync` (stable) instead of `m`. Read the date in
  `dailyNoteDate` through `propertyValueOf`.
- **I-57:**
  - Render `EntityRelationsField` for a `relations` property with an empty
    `targetStructure` (the picker then lists any structure and offers no "Create").
  - Both relation fields leave the current entity out of the picker's candidates. Pass
    the id down explicitly; name the prop to fit (e.g. `excludeIds` or `selfId`).
- Follow `CLAUDE.md` component style for anything you touch.

## Tests
There's no FE test runner. `pnpm build` and `pnpm lint`, and describe the browser checks.

## Out of scope
Relation dead refs (I-14, handled on the server). Relation add/remove ops (I-34). Any
registry change: no structure declares an any-structure or self-typed relation today,
so don't add one to demonstrate I-57.

## Done when
- `cd calcifer && pnpm build && pnpm lint` pass.
- Browser checks to report: creating a Note and a Tag from the mention menu (Enter, and
  a click) sends one `ResolveEntity` each and inserts one chip; pressing Enter twice
  quickly still sends one. The Todo tags picker and the daily-note "New" button still
  work.
- Say how the self-exclusion and the any-structure picker were checked (by reading, since
  no structure exercises them).
- `ISSUES.md`: move I-43, I-56 and I-57 to Resolved. For I-43, name the cause or say the
  guard is the fix.

## Commits
1. `fe: send one ResolveEntity per mention create (I-43)`
2. `fe: keep create and resolve callbacks stable (I-56)`
3. `fe: render any-structure relations and leave self out of pickers (I-57)`
4. `docs: resolve I-43, I-56 and I-57`
