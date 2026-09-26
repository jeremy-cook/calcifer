# T22 · FE model-layer duplicates

**Area:** fe · **Issues:** I-33 (D2, the remainder)

## Problem
Read I-33 in `ISSUES.md`. T15 already handled the sync parts (the Watch consumer,
`entitiesQuery`, raw `['entities']` keys); its note on I-33 lists what's left. Expected
leftovers:
- Timestamp-to-milliseconds conversion is hand-written in several places
  (`model/backlinks.ts`, `model/store.ts` `updatedAtMillis`, `model/todos.ts`).
  `timestampMs` from `@bufbuild/protobuf/wkt` does this.
- The delete path exists twice (`useDeleteEntity` and `deleteEntityImperative`).
- The create mutation exists twice (`useCreateEntity` / `useCreateDailyNote`, or
  whatever is left after T08).

## Requirements
- Use `timestampMs` everywhere a Timestamp is converted to milliseconds, and delete the
  hand-written versions.
- Keep exactly one implementation of delete: the hook wraps the imperative function, or
  vice versa. Do the same for create, where the two still share logic.
- Handle anything else listed in I-33's progress note.
- No behaviour change.

Tech lead notes (from T21):
- `useSetTodoStatus` (`model/todos.ts`, about l.180) builds a raw `{ case: 'select' }`.
  Use `usePropertyWriters().setSelect` from `model/store.ts` instead.
- Entity property reads still index the map directly in `model/todos.ts` (about l.42 and
  l.49) and `routes/e.$id.tsx` (about l.194). `model/store.ts` has a private
  `propertyOf` that reads through `Object.hasOwn`. Export one read helper and use it at
  all three sites.
- I-33's line numbers in `ISSUES.md` predate T15 and T21; locate by name, not line.

## Out of scope
Anything not listed in I-33.

## Done when
- `grep -rn "nanos / 1_000_000\|seconds) \* 1000" calcifer/src` finds nothing.
- One delete path and one create path, which you name in your report.
- `pnpm build` and `pnpm lint` pass.
- `ISSUES.md`: move I-33 to Resolved.

## Commits
1. `fe: use timestampMs and merge duplicate create/delete paths (I-33)`
2. `docs: resolve I-33`
