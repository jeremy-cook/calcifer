# Phase 6 Part 2 — Wire DateChip click and option-click

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 6.
> Predecessor: [`phase-6-part-1-calendar-layout.md`](./phase-6-part-1-calendar-layout.md) shipped in commit `5e601c6`.

## Context

The calendar page exists but is an island: nothing links to it, and it has no notion of "the date the user is looking at" beyond local component state. DateChips in rich-text bodies open the date picker on click — fine for re-editing, but there's no way to **explore** a date (its references, its journal, etc.) from the place a chip appears. Part 2 connects those two surfaces: a primary click on a DateChip becomes navigation, alt-click stays as re-edit. The `/calendar` URL also gains a search param so the navigation target is real.

This part is **navigation-only**. The "Date references" section keeps its hardcoded example row — wiring real backlinks is a separate piece, deferred until linkSync gets extended to recognize date chips.

## Goal

- Clicking a `DateChip` in any rich-text body navigates to `/calendar?date=$iso` for that chip's date.
- Alt/option-click re-opens the date picker (today's behavior, preserved).
- The calendar URL encodes the selected day in a search param: `/calendar?date=$iso`. Bare `/calendar` defaults to today.
- Toolbar `‹ / Today / ›` buttons and mini-calendar day clicks update the URL (no internal day state in `CalendarPage`).
- Sidebar **Calendar** link points at `/calendar` (no param) and lands on today.

## Why search param, not path param

A path-param shape (`/calendar/$iso`) was the first instinct, but TanStack's file-based router treats `calendar.$iso.tsx` as a nested child requiring a `calendar.tsx` parent. Adding a parent route just to host `<Outlet/>` is dead weight — and without one the plugin generates a broken `routeTree.gen.ts` and HMR enters a regenerate loop. A search param keeps the route flat, makes bare `/calendar` a real URL (good for sidebar / bookmarks), and is trivial to default to today.

## File changes

```
calcifer/src/
  routes/
    calendar.tsx              ← rewrite: validateSearch reads ?date=, falls back to todayIso
  components/calendar/
    CalendarPage.tsx          ← drop internal useState; accept `iso` prop;
                                onChange handlers call navigate({ to:'/calendar', search:{date} })
  layouts/sidebar/
    CalendarItem.tsx          ← drop truncate (cosmetic cleanup)
  editors/tiptap/components/date/
    DateChipView.tsx          ← detect altKey; primary click navigates with search param;
                                alt branches to picker
```

No new deps. No schema or store changes.

## Engineering detail

### `routes/calendar.tsx` (rewrite)

Keep a single flat `/calendar` route. Read the day from a `?date=$iso` search param; default to today when absent.

```tsx
import { createFileRoute } from '@tanstack/react-router'
import { CalendarPage } from '~/components/calendar/CalendarPage'
import { todayIso } from '~/model/dates'

interface CalendarSearch {
  date?: string
}

export const Route = createFileRoute('/calendar')({
  validateSearch: (search: Record<string, unknown>): CalendarSearch => ({
    date: typeof search.date === 'string' ? search.date : undefined,
  }),
  component: function CalendarRoute() {
    const { date } = Route.useSearch()
    return <CalendarPage iso={date ?? todayIso()} />
  },
})
```

If a malformed `date` slips through, `dateFromIso` returns `Invalid Date` and date-fns formatters render `'Invalid Date'` strings — visually obvious, not crashing. Good enough; revisit if it becomes an issue.

### `components/calendar/CalendarPage.tsx`

Drops internal state. Becomes a pure `iso` consumer; the route owns the source of truth.

```tsx
interface CalendarPageProps {
  iso: string
}

export function CalendarPage({ iso }: CalendarPageProps) {
  const navigate = useNavigate()

  const goTo = (next: string) =>
    void navigate({ to: '/calendar', search: { date: next }, replace: true })

  return (
    <div className="flex h-full flex-col">
      <CalendarToolbar iso={iso} onChangeIso={goTo} />
      <div className="grid min-h-0 flex-1 grid-cols-[1fr_320px]">
        <DayView iso={iso} />
        <MiniCalendar iso={iso} onSelect={goTo} />
      </div>
    </div>
  )
}
```

`replace: true` keeps rapid prev/next clicks from bloating browser history. `CalendarToolbar` and `MiniCalendar` stay unchanged — same `iso` + change-callback prop shape. They don't need to know about routing.

### `editors/tiptap/components/date/DateChipView.tsx`

Add `useNavigate` and an alt-key branch. Keep the existing re-edit behavior on alt-click.

```tsx
import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react'
import { useNavigate } from '@tanstack/react-router'
import { formatDate } from '~/lib/tiptap-extension-date'
import { showDatePicker } from './showDatePicker'

export function DateChipView({ node, updateAttributes, editor }: NodeViewProps) {
  const { date } = node.attrs as { date: string }
  const navigate = useNavigate()

  function handleClick(e: React.MouseEvent<HTMLSpanElement>) {
    if (e.altKey) {
      showDatePicker(editor, {
        initialDate: date,
        referenceEl: e.currentTarget,
        onConfirm: (newDate) => updateAttributes({ date: newDate }),
      })
      return
    }
    void navigate({ to: '/calendar', search: { date } })
  }

  return (
    <NodeViewWrapper
      as="span"
      className="date-chip"
      onClick={handleClick}
      title="Open in calendar (alt-click to edit)"
      contentEditable={false}
    >
      {formatDate(date)}
    </NodeViewWrapper>
  )
}
```

The native `title` tooltip surfaces the alt-click affordance — small, but enough for discovery. No new keyboard handling: chip is `contentEditable={false}` and not focusable, so keyboard activation is a non-goal here.

### `layouts/sidebar/CalendarItem.tsx`

Plain `<Link to="/calendar">` — the route defaults to today when no search param is present, so the sidebar doesn't need to compute it. Active highlight uses default exact-match (any `/calendar?date=...` URL is still the same path, so the item lights up).

Also drop the `truncate` class on the label span — pre-existing defensive styling that's never going to fire on the static string "Calendar".

## What's NOT in Part 2

- No real "Date references" data — section keeps its hardcoded example row. Wiring requires extending `linkSync.ts` to scan `dateChip` nodes and a `useDateBacklinks(iso)` hook against a synthetic `date:${iso}` target. Separate piece.
- No DailyNote creation/lookup. The "Daily note" section stays a placeholder.
- No NL parsing in the toolbar's calendar-icon button (Part 3).
- No keyboard shortcut for navigation (cmd-click for new tab is whatever the browser does by default — DateChip uses a span with onClick, so cmd-click just navigates in-place; revisit if it bites).
- No DailyNote title rendering on `/e/$id` for date entities (DailyNote isn't an entity yet).

## Verification

`pnpm dev`, then:

1. **Chip → calendar**: Open any entity page with a DateChip in its body. Click the chip. URL changes to `/calendar?date=<chip-iso>`; calendar page renders that day's heading.
2. **Alt-click re-edit**: Hold alt and click the same chip. Date picker opens at the chip's position; selecting a new date updates the chip text in place; URL does not change.
3. **Toolbar nav updates URL**: From `/calendar?date=2026-04-27`, click `›` — URL becomes `/calendar?date=2026-04-28`. Click `Today` — URL becomes `/calendar?date=<today>`. Refresh holds the day.
4. **Mini-calendar nav updates URL**: Click a different day in the mini-calendar. URL updates; day view follows.
5. **Sidebar link**: Sidebar **Calendar** link goes to `/calendar` and shows today. While on any `/calendar?date=...`, the sidebar item shows the active highlight.
6. **No regressions**: Mention insertion still works on entity pages; backlinks still render; entity routes still navigate.

## Definition of done

- DateChip click navigates to `/calendar?date=$iso` for that chip's date.
- Alt-click on a DateChip preserves the existing re-edit picker behavior.
- `CalendarPage` no longer holds an internal day state; route search param is the source of truth.
- Sidebar **Calendar** link works and stays highlighted across all `/calendar?date=...` URLs.
- No regressions on entity pages, mention insertion, backlinks panel.

## Performance note

During testing, rapid prev/next clicks felt sluggish in `pnpm dev`. A Firefox profile traced 62% of CPU to the cycle collector and only ~6% to React work, with `runWithFiberInDEV` overhead visible — a dev-mode artifact. Production build feels smooth; no code changes warranted. `replace: true` on the navigate call is kept regardless to avoid history bloat.
