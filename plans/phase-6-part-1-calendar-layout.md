# Phase 6 Part 1 — Calendar page layout

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 6.

The full Phase 6 (DateRef + DailyNote + calendar + NL parsing) is too big for one bite. Break into three parts; each is independently shippable. This document covers **Part 1 only**. Parts 2 and 3 are sketched at the bottom so the direction is visible, but their detailed design is deferred until Part 1 lands.

## Context

`/calendar` today is a placeholder. Replace it with a Capacities-style calendar page (per the reference screenshot): a two-pane day view on the left and a mini-month picker on the right, with view-mode tabs and prev/today/next navigation across the top. The page becomes the new "home" for everything date-shaped — day journal, list of cross-references to the day, and a footer of entities created on the day.

**Part 1 is layout only.** No wiring to the entity store, no real DailyNote, no real backlinks. Make it look right; make the structure exist.

## Goal

Replace the `/calendar` placeholder with a static layout matching the reference screenshot. The page reads the current local date once on mount and renders against it. Nothing is created, nothing is fetched from the store, nothing is interactive beyond local toolbar/mini-calendar navigation between days.

## Anatomy

```
┌─────────────────────────────────────────────────────────────────┐
│         Month   Week   Day                  ‹  Today  ›   📅    │  ← CalendarToolbar
├─────────────────────────────────────┬───────────────────────────┤
│                                     │                           │
│  Saturday                           │  ‹    April ▾  2026 ▾   › │  ← MiniCalendar
│  April 25, 2026   Week 17           │                           │
│                                     │  Mo Tu We Th Fr Sa Su     │
│  ─────────────────────────────────  │  30 31  1  2  3  4  5     │
│                                     │   6  7  8  9 10 11 12     │
│  Daily note          ⤢   …          │  13 14 15 16 17 18 19     │
│   🏷 Tags                           │  20 21 22 23 24 (25) 26   │
│   Text                              │  27 28 29 30  1  2  3     │
│  ─────────────────────────────────  │   4  5  6  7  8  9 10     │
│                                     │                           │
│  Date references                    │                           │
│   April 25, 2026                    │                           │
│   ▸ Eric Rules    👤 Person  …      │                           │
│       April 25, 2026                │                           │
│  ─────────────────────────────────  │                           │
│                                     │                           │
│  ⌘ Created on This Day (0)          │                           │
└─────────────────────────────────────┴───────────────────────────┘
```

Match the screenshot's vertical rhythm — section dividers, padding, muted secondary text, accent-colored weekday label.

## File changes

```
calcifer/src/
  routes/
    calendar.tsx                  ← rewrite: thin route wrapper that mounts <CalendarPage/>
  components/calendar/            ← NEW folder
    CalendarPage.tsx              ← top-level layout: toolbar + two-pane split
    CalendarToolbar.tsx           ← view-mode tabs + prev/today/next + calendar icon
    DayView.tsx                   ← left pane: date heading + sections
    DayHeading.tsx                ← weekday + big date + week-number sub-component
    DailyNoteSection.tsx          ← "Daily note" heading + placeholder body
    DateReferencesSection.tsx     ← "Date references" heading + placeholder list
    CreatedAt.tsx                 ← footer with cmd icon and zero-state count
    MiniCalendar.tsx              ← right pane: shadcn Calendar wrapper
  model/
    dates.ts                      ← NEW (small for now): todayIso, isoFromDate,
                                    dateFromIso, formatLongDate, formatWeekday, weekNumber
```

`date-fns` is already in the project (used in `s.$structureType.tsx` for `formatDistanceToNow`) — reuse for week-of-year and weekday formatting. No new deps for Part 1.

## Engineering detail

### `model/dates.ts` (Part 1 scope)

```ts
export function todayIso(): string                       // local YYYY-MM-DD
export function isoFromDate(d: Date): string
export function dateFromIso(iso: string): Date           // local-midnight Date
export function formatLongDate(iso: string): string      // 'April 25, 2026'
export function formatWeekday(iso: string): string       // 'Saturday'
export function weekNumber(iso: string): number          // 17 (ISO week, via date-fns getISOWeek)
```

Keep this file minimal in Part 1 — it grows to hold `parseNaturalDate`, `findDailyNoteByIso`, etc. in later parts.

### `routes/calendar.tsx`

```tsx
import { createFileRoute } from '@tanstack/react-router'
import { CalendarPage } from '~/components/calendar/CalendarPage'

export const Route = createFileRoute('/calendar')({
  component: CalendarPage,
})
```

URL stays as `/calendar` — no params yet. The page's "current day" is internal state initialised to `todayIso()`. (In Part 2, we'll either move state into a search param or accept an ISO route param so `DateChip` clicks can deep-link.)

### `CalendarPage.tsx`

Top-level layout:

```tsx
export function CalendarPage() {
  const [iso, setIso] = useState(todayIso())

  return (
    <div className="flex h-full flex-col">
      <CalendarToolbar iso={iso} onChangeIso={setIso} />
      <div className="grid min-h-0 flex-1 grid-cols-[1fr_320px]">
        <DayView iso={iso} />
        <MiniCalendar iso={iso} onSelect={setIso} />
      </div>
    </div>
  )
}
```

Two-column grid: flexible left pane, fixed-width right pane (~320px). The split is non-resizable in Part 1; we can wrap in `Resizable` later if needed.

### `CalendarToolbar.tsx`

```tsx
interface CalendarToolbarProps {
  iso: string
  onChangeIso: (iso: string) => void
}
```

- Centered tab group: **Month / Week / Day** — only "Day" is enabled (highlighted). The others render as disabled buttons; tooltip "Coming soon". Visible affordances for later parts.
- Right side: `‹` / **Today** / `›` button group, then a small calendar-popover icon button (cosmetic — opens nothing in Part 1).
- `‹` and `›` shift `iso` by ±1 day; **Today** sets it to `todayIso()`. This is the only working interaction in the toolbar for Part 1.

### `DayView.tsx`

Stack of sub-components, separated by `border-t border-border`:

```tsx
<div className="flex flex-col gap-6 px-12 py-8">
  <DayHeading iso={iso} />
  <Divider />
  <DailyNoteSection />
  <Divider />
  <DateReferencesSection />
  <Divider />
  <CreatedAt />
</div>
```

**`DayHeading`** — accent-colored weekday over a large bold date, with muted week number to the right of the date.

```tsx
<div>
  <p className="text-[var(--chart-1)] text-sm font-medium">{formatWeekday(iso)}</p>
  <div className="flex items-baseline gap-3">
    <h1 className="text-4xl font-bold tracking-tight">{formatLongDate(iso)}</h1>
    <span className="text-sm text-muted-foreground">Week {weekNumber(iso)}</span>
  </div>
</div>
```

The accent color matches the Note Structure's color today (`var(--chart-1)`) — mirrors how Capacities tints the weekday by the entity type's color. Revisit this once we settle on a "DailyNote" color.

**`DailyNoteSection`** — section heading "Daily note" with a small expand icon and `…` action button on the right; content area renders an empty editor-shaped placeholder (a muted `Tags` chip placeholder and a muted `Text` placeholder) — purely visual for Part 1.

```tsx
<section className="flex flex-col gap-3">
  <header className="flex items-center justify-between">
    <h2 className="text-base font-semibold">Daily note</h2>
    <div className="flex items-center gap-1">
      <button aria-label="Expand"><ArrowsOutSimpleIcon className="size-4" /></button>
      <button aria-label="More"><DotsThreeIcon className="size-4" /></button>
    </div>
  </header>
  <div className="flex flex-col gap-2 text-muted-foreground">
    <button className="flex items-center gap-2 self-start rounded-md px-2 py-1 hover:bg-muted">
      <TagIcon className="size-4" /> Tags
    </button>
    <p className="px-2 text-sm">Text</p>
  </div>
</section>
```

**`DateReferencesSection`** — heading "Date references" with one cosmetic example row replicating the screenshot's "Eric Rules / Person / April 25, 2026" sub-row. Part 1 hardcodes this example to validate the layout. Part 2/later replaces it with real data via `useBacklinks(...)`.

**`CreatedAt`** — single line: a small `⌘` icon followed by "Created on This Day (0)". Cosmetic placeholder; no real query in Part 1.

### `MiniCalendar.tsx`

```tsx
interface MiniCalendarProps {
  iso: string
  onSelect: (iso: string) => void
}
```

Use the existing shadcn `Calendar` (`~/components/ui/calendar`, already imported by `DatePickerPopup.tsx`). It wraps `react-day-picker`, which handles month navigation, day-of-week headers, today highlight, and selection out of the box.

```tsx
import { Calendar } from '~/components/ui/calendar'

export function MiniCalendar({ iso, onSelect }: MiniCalendarProps) {
  const selected = dateFromIso(iso)
  const handleSelect = (date: Date | undefined) => {
    if (date) onSelect(isoFromDate(date))
  }
  return (
    <div className="border-l border-border p-4">
      <Calendar
        mode="single"
        selected={selected}
        onSelect={handleSelect}
        month={selected}
        weekStartsOn={1}
        captionLayout="dropdown"
        showOutsideDays
      />
    </div>
  )
}
```

- `mode="single"` + `selected/onSelect` gives single-day selection.
- `weekStartsOn={1}` matches the screenshot's Mo–Su layout.
- `captionLayout="dropdown"` exposes month/year dropdowns in the caption (react-day-picker built-in). Header chevrons for prev/next month come for free.
- `month={selected}` keeps the visible month synced with the externally-selected day, so toolbar `Today` jumps the mini-calendar back to today's month.
- `showOutsideDays` matches the screenshot's dimmed leading/trailing days.

Part 1 accepts whatever native styling the shadcn Calendar ships with — Sundays may not render in red, and the today-circle visual may differ slightly from the screenshot. That's fine; functional parity is the bar. If we want to tighten the look later, react-day-picker exposes `modifiers` and `modifiersClassNames` for cell styling.

## What's NOT in Part 1

- No DailyNote entity creation, no entity-store reads (other than the static example row).
- No real backlinks data; the "Date references" section shows a single hardcoded example to validate the layout.
- No NL parse in the toolbar's calendar-icon button (it's cosmetic).
- No mini-calendar custom-styled markers.
- No deep-linking: URL stays `/calendar` regardless of which day is selected internally. Refresh resets to today. (We'll move to a path/search param in Part 2 when chip navigation needs it.)
- No keyboard shortcuts.
- View-mode tabs other than "Day" are inert.

## Verification

`pnpm dev`, navigate to `/calendar`. Confirm visually:

- Page matches the reference screenshot in structure, spacing, and content slots.
- Weekday in accent color, big bold date, muted "Week N" reading correctly for today.
- "Daily note" / "Date references" / "Created on This Day (0)" sections render with their headings and example/placeholder content.
- Mini calendar shows the current month with today highlighted; out-of-month days dimmed (per react-day-picker defaults).
- Toolbar `‹` / `›` shifts the day view by ±1 day; "Today" returns to today and snaps the mini-calendar back to today's month. Mini-calendar's chevrons / dropdowns navigate months. Mini-calendar day clicks update the day view.
- Other view-mode tabs are visibly disabled.

## Definition of done for Part 1

- `/calendar` renders the new layout end to end.
- No regressions on other pages (sidebar, entity pages, mention insertion).
- Code organized in `components/calendar/` per CLAUDE.md style: one component per file, named props interfaces, constants above the return inside the function, sub-components for distinct logical sections.

---

## Part 2 — Wire DateChip click and option-click (sketch only)

**Goal:** clicking a `DateChip` in any rich-text body navigates to the calendar page anchored on that chip's date. Option/alt-click re-opens the date picker (current behavior preserved).

**Likely shape:**
- Move the calendar route to `/calendar/$iso` (or accept `?date=$iso` search param) so chip clicks can deep-link. `CalendarPage` reads `iso` from the route instead of internal state.
- `DateChipView.handleClick` — primary click: `router.navigate({ to: '/calendar/$iso', params: { iso: date } })`. Alt-key branch: keep the existing `showDatePicker` re-edit flow.
- Toolbar `‹ / Today / ›` and mini-calendar selection update the URL via `navigate`, not local state.
- Sidebar `/calendar` link points at today's ISO (or stays as `/calendar` and redirects).

Open question for Part 2: do we also want to start populating real "Date references" data here (i.e. wire the section to `useBacklinks(date:${iso})` against `linkSync` — which would require extending `linkSync.ts` to recognize `dateChip` nodes and synthesize a `date:${iso}` target)? Or hold that until a later part? Decide when we get there.

## Part 3 — Natural-language parsing (sketch only)

**Goal:** the user can type "today" / "tomorrow" / "next monday" and resolve to an ISO date.

**Likely shape:**
- Add `chrono-node` dep.
- `parseNaturalDate(input: string): string | null` in `~/model/dates.ts` (forwardDate: true).
- Plumb into `DatePickerPopup` as a text input above the calendar. Parsed input updates the calendar selection; user still hits Insert to confirm.
- Optionally also plumb into the calendar toolbar (a "jump to" input that navigates to the parsed date).

---

## Out of scope for Phase 6 entirely (to revisit later)

- Whether `DateRef` should be a real entity or stay virtual (synthetic `date:${iso}` ids). Undecided; Part 1 doesn't depend on this and Part 2 will force the question.
- Sidebar **Today** button. Easy to add when DailyNote wiring lands.
- Month/Week view modes. Day view only for now.
- DailyNote read-only title rendering on `/e/$id`. Land alongside whatever part actually creates DailyNotes.
