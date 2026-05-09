# Phase 6 Part 7 — Edit DailyNote date

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 6.
> Predecessor: [`phase-6-part-6-slash-date-shortcuts.md`](./phase-6-part-6-slash-date-shortcuts.md).

## Context

Phase 6 Part 4 deferred "moving a journal entry to another day." A DailyNote's date prop was created `editable: false` since Part 3, the title was hard-derived from the date at creation, and the entity page only knew how to render richtext properties. Together that meant a DailyNote was effectively pinned to its creation day — if you accidentally created a journal under the wrong day, the only fix was delete + recreate (and you'd lose the body).

## Goal

From the DailyNote's entity page, change its date to any other day. If the target day already has its own DailyNote, refuse the move with an inline message; the user can manually delete one of the two and retry. No collision merging; no destructive overwrite.

## Design

### Structure metadata

`calcifer/src/model/structures.ts` — drop `editable: false` from the DailyNote `date` prop. `PropertyDef.editable` has no consumer today; the edit is a signal of intent. Only DailyNote owns a date prop, so no other Structure is affected.

### Store action

Add `moveDailyNote(id, newIso): boolean` to the entity store. Returns `true` on success (or no-op if same iso), `false` on collision so the UI can surface an error.

```ts
moveDailyNote: (id, newIso) => {
  const current = get().entities[id]
  if (!current || current.structureType !== 'DailyNote') return false
  const dateProp = current.properties.find((p) => p.id === 'date')
  const v = dateProp?.value?.value
  if (v?.case === 'date' && v.value === newIso) return true
  const occupant = dailyNoteByDate(get().entities, newIso)
  if (occupant && occupant.id !== id) return false
  // rewrite date property + re-derive title
  const properties = current.properties.map((p) =>
    p.id === 'date'
      ? createMessage(PropertySchema, {
          id: 'date',
          value: createMessage(PropertyValueSchema, { value: { case: 'date', value: newIso } }),
        })
      : p,
  )
  const next = createMessage(EntitySchema, {
    ...current,
    properties,
    title: formatLongDate(newIso),
    updatedAt: timestampNow(),
  })
  set({ entities: { ...get().entities, [id]: next } })
  return true
}
```

The title is re-derived rather than left at the old `formatLongDate(oldIso)` — DailyNote's title is structurally a function of its date, so the two must move together. Other entities' `referenced_dates` are not touched: those are isos, not entity ids, so they're independent of which DailyNote currently lives at a given iso.

### `EntityDateField` component

New file `calcifer/src/components/entity/EntityDateField.tsx`. A labelled row with a popover-anchored shadcn Calendar. On select, calls `onChange(newIso): boolean`. Returns false → renders an inline error inside the popover; user dismisses by closing. Same iso → close without calling onChange.

```ts
interface EntityDateFieldProps {
  label: string
  iso: string
  onChange: (newIso: string) => boolean
  collisionMessage?: string
}
```

Generic in shape; the parent decides what `onChange` does. For DailyNote, `onChange` wraps `moveDailyNote(entity.id, iso)`.

### Entity page wiring

`calcifer/src/routes/e.$id.tsx` — extend `EntityProperties.renderProperty` switch with `case 'date'`. For DailyNote, render `EntityDateField` wired to the store's `moveDailyNote`. For any other structure with a date prop (none today), fall through to null.

The date row renders between the header and the richtext content because DailyNote's properties are ordered `[date, content]` in `structures.ts`.

## File changes

```
calcifer/src/
  model/
    structures.ts                    ← drop `editable: false` on DailyNote.date
    store.ts                         ← add moveDailyNote(id, newIso): boolean
  components/
    entity/
      EntityDateField.tsx            ← new: labelled popover + Calendar with collision error
  routes/
    e.$id.tsx                        ← extend property-render switch with `case 'date'` for DailyNote
```

No proto changes. No new deps.

## Out of scope

- Calendar surface affordance for moving a DailyNote. The `DailyNoteSection`'s expand-to-page button already gets the user to `/e/$id` where the move lives. One entry point keeps the UI surface small.
- Generic editable date on non-DailyNote structures. No structure has one today; revisit when the first does.
- Merging two DailyNotes when target is occupied. Destructive; not worth the modal scaffolding for an edge case.
- Indicator on the calendar that a DailyNote was recently moved. Audit/undo is a separate concern.

## Verification

`pnpm dev`, then:

1. **Same-day no-op** — Open a DailyNote's `/e/$id`, click the date, pick the same day. Popover closes; no entity churn (`updatedAt` unchanged in localStorage).
2. **Move to empty day** — Pick a different day with no existing DailyNote. Popover closes, title updates to the new long-form date, `dailyNoteByDate(oldIso)` returns undefined, `dailyNoteByDate(newIso)` returns this entity.
3. **Collision** — A and B are DailyNotes for two different days. Open A's page, try to move it to B's day. Popover stays open, "A daily note already exists for that day." appears in red, A's date is unchanged.
4. **Calendar reflects move** — After step 2, navigate to `/calendar?date=$oldIso` → "+ New" empty state; `/calendar?date=$newIso` → the journal renders with its body intact.
5. **Mini-calendar marker follows** — The dot disappears from the old day and appears on the new day (since `daysWithContent` derives from current state).
6. **Body intact** — Backlinks/referenced_dates/content all survive the move (entity id is unchanged).
7. **No regressions** — Note/Tag entity pages render unchanged (no date prop); empty-DailyNote pruning still works on the calendar surface.

## Definition of done

- DailyNote's date prop is editable through `EntityDateField` on `/e/$id`.
- `moveDailyNote(id, newIso)` is the only path that changes the date; collisions are refused; title re-derives on success.
- Calendar surfaces (`DailyNoteSection`, `MiniCalendar` markers, `entitiesByDate`) reflect the new state without code changes — they were already iso-driven.
