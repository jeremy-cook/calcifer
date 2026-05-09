# Phase 6 Part 5 — Mini-calendar markers

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 6.
> Predecessor: [`phase-6-part-4-date-references.md`](./phase-6-part-4-date-references.md) (`2630d21`).

## Context

Phase 6 Parts 1–4 shipped the calendar surface, DateChip → calendar nav, DailyNote click-to-create with empty-prune, and the real "Date references" right-column panel backed by `Entity.referenced_dates`. The mini-calendar still had no visual indication of which days actually have content — every day looked identical, so users had to click around to discover anything.

This is the smallest of the deferred Phase 6 items. With `referenced_dates` already maintained by `linkSync` and `dailyNoteByDate` already a selector, the data is right there; the work is wiring it into DayPicker's `modifiers` and adding the dot styling.

## Goal

A single small dot below each day number on the mini-calendar when that day either has a `DailyNote` entity or appears as a referenced date in any entity's `referenced_dates`. No new persisted state; pure derivation.

## Design

### Selector: `daysWithContent(entities)`

Added to `calcifer/src/model/store.ts`, next to `dailyNoteByDate` / `entitiesByDate`:

```ts
export function daysWithContent(entities: Record<string, Entity>): Set<string> {
  const set = new Set<string>()
  for (const e of Object.values(entities)) {
    if (e.structureType === 'DailyNote') {
      const dateProp = e.properties.find((p) => p.id === 'date')
      const v = dateProp?.value?.value
      if (v?.case === 'date') set.add(v.value)
    }
    for (const iso of e.referencedDates) set.add(iso)
  }
  return set
}
```

One pass over entities. Any DailyNote counts (no `isRichTextEmpty` check) — Part 4's auto-prune keeps empty journals from sticking around long enough to matter.

### Wiring through `MiniCalendar`

`MiniCalendar` is the only consumer. Subscribes to `entities` (stable reference) and derives the set via `useMemo` — same pattern `useBacklinks` and `DateReferencesSection` use to dodge the snapshot-identity flap noted in Part 4.

```tsx
// calcifer/src/components/calendar/MiniCalendar.tsx
const entities = useEntityStore((s) => s.entities)
const marked = useMemo(() => daysWithContent(entities), [entities])

return (
  <Calendar
    /* existing props */
    modifiers={{ hasContent: (date) => marked.has(isoFromDate(date)) }}
    modifiersClassNames={{ hasContent: HAS_CONTENT_CLASSES }}
  />
)
```

DayPicker forwards `modifiers` / `modifiersClassNames` natively; the shadcn `Calendar` wrapper in `calcifer/src/components/ui/calendar.tsx` already spreads `{...props}` through.

### Dot styling

The dot is a `::after` pseudo-element on the day cell, scoped via the `modifiersClassNames` map so the styling stays colocated with MiniCalendar (and won't leak into the entity-page DatePickerPopup).

```ts
const HAS_CONTENT_CLASSES =
  'after:pointer-events-none after:absolute after:bottom-1 after:left-1/2 after:z-20 ' +
  'after:size-1 after:-translate-x-1/2 after:rounded-full after:bg-foreground ' +
  'after:opacity-60 after:content-[""]'
```

`after:z-20` is load-bearing: `CalendarDayButton` has `relative isolate z-10`, so without an explicit z-index on the pseudo it would sit behind the button's `bg-primary` on selected days. The `day` className in MiniCalendar already sets `relative` (line 39), so positioning works without further changes.

## File changes

```
calcifer/src/
  model/
    store.ts                        ← add daysWithContent(entities)
  components/
    calendar/
      MiniCalendar.tsx              ← subscribe + derive marked set; pass modifiers + modifiersClassNames
```

No proto changes. No new deps. No new files.

## Out of scope

- Distinguishing journal vs chip-reference dots (single unified dot — call we made on this part).
- Marker for "today has nothing yet" — bare today already gets `bg-muted` via the `today` modifier.
- Markers in the embedded DatePickerPopup used by date-chip insertion (`calcifer/src/editors/tiptap/components/date/DatePickerPopup.tsx`). That picker is for choosing a date to reference, not for browsing existing content; the modifier is scoped to MiniCalendar's wiring so it won't leak.
- Indexed lookup. `daysWithContent` is O(N) over entities; for personal-scale KB this is fine, and it's recomputed only when `entities` changes.

## Verification

`pnpm dev`, then:

1. **Empty calendar** — Fresh state (or a month with no content). MiniCalendar shows no dots.
2. **DailyNote dot** — Click into a calendar day, type a sentence, navigate to the next month and back. Original day shows a dot below the number.
3. **Chip reference dot** — On a Note, insert a DateChip for a different day. Wait for debounce. Open the calendar, navigate to the chip's month — that day shows a dot.
4. **Both kinds same day** — A day with both a DailyNote and a chip reference shows a single dot (not two).
5. **Cross-month visibility** — Navigate the calendar forward/back; dots render on outside-days too (the `showOutsideDays` slot still receives modifiers).
6. **Selected/today contrast** — Click a marked day → dot still visible behind the primary background. Today (`bg-muted`) when marked → dot still visible.
7. **Pruning removes dot** — Click into an empty day, leave without typing. Dot should NOT appear (Part 4 prunes the empty DailyNote on iso-change).
8. **No regressions** — DateChip insertion / re-edit, DailyNoteSection click-to-create + delete + expand, DateReferencesSection list, calendar nav arrows, sidebar Calendar entry.

## Definition of done

- `daysWithContent(entities)` returns the union of DailyNote dates and every iso appearing in any `referenced_dates`.
- MiniCalendar shows a single subtle dot below the day number on every day in that set, on the current and outside-day cells, in both light and dark themes.
- All verification steps pass; no regressions on Phase 6 Parts 1–4 surfaces.
