# T03 · Sort daily notes by date, not name

**Area:** fe · **Issues:** I-19 (C4)

## Problem
`listByStructure` in `calcifer/src/model/store.ts` sorts DailyNotes by `name` descending
with `localeCompare`. Names are long dates ("June 13, 2026"), so the order is alphabetical
by month, and "June 9" comes before "June 13".

## Requirements
- Sort DailyNotes by their `date` property (ISO `yyyy-MM-dd`, so a string compare is
  chronological), newest first. Use the existing `dailyNoteDate(entity)` helper in the
  same file.
- A DailyNote with no date sorts last. Break ties by `id` so the order is stable.
- Leave other structures' sort (by `updatedAt`) unchanged.

## Out of scope
The timestamp helper duplication (`updatedAtMillis`; that's T22). Any other sort.

## Done when
- `listByStructure(entities, 'DailyNote')` orders by date descending.
- `pnpm build` and `pnpm lint` pass.
- Report a browser check: the DailyNote list shows June 13 above June 9.

## Commit
`fe: sort daily notes by date, not name (I-19)`. Then a second commit moving I-19 to
**Resolved** in `ISSUES.md` (house format: date 2026-mm-dd and a one-line note), subject
`docs: resolve I-19 in ISSUES.md`.
