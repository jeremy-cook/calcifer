# Phase 6 Part 3 — DailyNote wiring

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 6.
> Predecessors: [`phase-6-part-1-calendar-layout.md`](./phase-6-part-1-calendar-layout.md) (`5e601c6`), [`phase-6-part-2-datechip-navigation.md`](./phase-6-part-2-datechip-navigation.md) (`527231d`).

## Context

The calendar page exists and the DateChip routes into it, but the "Daily note" section on the calendar page is still cosmetic — a static "Tags" chip and a "Text" placeholder. There is no real journal entry for any day.

The user wants the calendar's Daily note section to become a real editable `DailyNote` entity for the displayed date. The entry point is the section itself: navigate to `/calendar` (today, via the sidebar) or `/calendar?date=$iso` (via a chip), then click the placeholder to start writing. There is no separate "Today" button — the sidebar Calendar link already lands on today.

`DailyNote` is already registered as a Structure (`creatable: false`, `mentionable: false`, one `content` richtext property) but has no way to be created from the UI and no way to be looked up by date. Part 3 closes both gaps and replaces the cosmetic section with a real editor.

DateRef and real "Date references" backlinks remain deferred — a date chip in some other note can't yet link to a DailyNote. For Part 3 the chip → calendar → daily-note flow is the value.

## Goal

- Visiting `/calendar?date=$iso` and clicking the Daily note section creates a `DailyNote` entity for that date and mounts an editable richtext body.
- A second visit to the same date shows the existing DailyNote with its content; the section is the live editor.
- The DailyNote's title is set to the formatted date on creation (e.g. `"April 25, 2026"`) and is non-editable on `/e/$id`.
- The expand icon in the section header navigates to `/e/{id}` once a DailyNote exists.

## Design

### Storage: how a DailyNote knows its date

A DailyNote's day is a **calendar day**, not an instant. The existing `PropertyValue.date_value: Timestamp` case represents an instant and would force a local-midnight anchor on every read/write — timezone-prone and not what we mean. Nothing in the app currently uses `date_value`, so **replace** it with a calendar-day string at the same field number rather than adding a parallel case:

```proto
// proto/calcifer/v1/entities.proto
message PropertyValue {
  oneof value {
    string text = 1;
    double number = 2;
    string date = 3;              // CHANGED: was `Timestamp date_value`. "yyyy-MM-dd".
    string select = 4;
    EntityRef relation = 5;
    RichTextRef richtext = 6;
  }
}
```

Run `buf generate` to regenerate TS types under `gen/ts/`. The TS-generated case becomes `{ case: 'date', value: '2026-04-25' }`. The existing `'date'` PropertyDef type label now matches a real proto case (it didn't before — it was a generated TS alias for the dropped `dateValue` case nobody read).

Register a `date` property on `DailyNote`:

```ts
DailyNote: {
  ...
  properties: [
    { id: 'date', type: 'date', editable: false },
    { id: 'content', type: 'richtext' },
  ],
  title: { editable: false },
  creatable: false,    // see below
  mentionable: false,
}
```

`EntityProperties` in `routes/e.$id.tsx` only switches on `richtext` today; the `date` case falls through to the `default: return null` branch, so the date stays invisible on `/e/$id` — metadata, not user chrome. `Entity.title` is set to `formatLongDate(iso)` at creation; the human-readable label and the lookup key are independent.

### What `creatable: false` means for DailyNote (and why it stays)

`creatable: false` doesn't mean "this structure can never be created" — it controls one specific UI: the sidebar **New** button reads `CREATABLE_STRUCTURES` (in `structures.ts`) to populate its menu, and `createEntity`'s type signature excludes non-creatable structures. The flag's real meaning is "no zero-arg create button — needs context."

DailyNote keeps `creatable: false` because creating a DailyNote requires a date. There's no sensible "New DailyNote" sidebar action — the user picks a date by navigating the calendar, then clicks the empty section. The dedicated `createDailyNote(iso: string)` helper in the store is the only create path; it takes the date as a required argument.

`isTitleEditable()` already exists in `structures.ts` but was previously dead code — Part 3 plugs it into `EntityHeader`.

### Store: creation and lookup

Two additions:

1. **`createDailyNote(iso: string): Entity`** on `useEntityStore` — dedicated helper. Allocates id, sets `title = formatLongDate(iso)`, materializes the `date` and `content` (richtext + empty doc) properties, writes to the store, returns the entity. Mirrors `createEntity`'s shape but is DailyNote-specific so we don't have to lift the `Exclude<StructureType, 'DailyNote'>` constraint on the generic creator.

2. **`dailyNoteByDate(entities, iso): Entity | undefined`** — free selector that iterates `entities`, matches `structureType === 'DailyNote'` AND the `date` property's value === iso. O(N), fine for a personal KB. Consumed via `useEntityStore((s) => dailyNoteByDate(s.entities, iso))` so the section re-renders on entity create/delete naturally, no extra plumbing.

### Reuse: extract `EntityRichTextField`

The richtext field wrapper inside `routes/e.$id.tsx` — which loads a doc by `RichTextRef`, debounces writes, and calls `syncLinksFromDoc` — is exactly what `DailyNoteSection` needs. Extract it into a shared component:

```
src/components/entity/EntityRichTextField.tsx   ← NEW
```

Export `EntityRichTextField` and the `RICHTEXT_DEBOUNCE_MS` constant. Add an `autoFocus?: boolean` prop that pipes through to `TiptapEditor` (which gains the same prop, threading it into `useEditor({ autofocus: autoFocus ? 'end' : false })`). Update `routes/e.$id.tsx` to import from there. `DailyNoteSection` mounts the same component bound to the DailyNote's content `RichTextRef`.

### DailyNoteSection rewrite

```tsx
interface DailyNoteSectionProps {
  iso: string
}

export function DailyNoteSection({ iso }: DailyNoteSectionProps) {
  const dailyNote = useEntityStore((s) => dailyNoteByDate(s.entities, iso))
  // ... renders header (title "Daily note" + actions) and body
}
```

Two body states:

- **No DailyNote yet** — render a clickable empty-state ("New" ghost button). On click, call `createDailyNote(iso)` from the store; the selector flips and the next render mounts the editor with `autoFocus`. Eager creation: clicking commits an entity, even if the user types nothing. Stray empty DailyNotes are an acceptable trade-off for Part 3 — easy to prune later.
- **Has DailyNote** — render `EntityRichTextField` bound to the DailyNote's `content` property's `RichTextRef`. Same component that powers `/e/$id`, so behavior (linkSync, debouncing, mention/slash menus) is identical. `hideToolbar` is set so the calendar's section reads as a journal field, not a full editor.

`autoFocus` only fires for the day the user just clicked into — driven by an `autoFocusKey: string | null` state that the section adjusts during render (resets when `iso` changes), so navigating to a *different* day with an existing DailyNote doesn't steal focus from elsewhere.

Header:
- "Daily note" title and two icon buttons on the right (visible only when a DailyNote exists):
  - **Expand** — navigates to `/e/{dailyNote.id}`.
  - **Delete** — opens an `AlertDialog` confirmation; on confirm, calls `deleteEntity(id)` and the section flips back to the empty state.
- The "..." menu from the original mockup is dropped.
- Drop the cosmetic "Tags" chip from the placeholder. Tag-on-entity affordances are a separate concern; the editor body itself already supports `#tag` mentions via the existing extension.

`DayView.tsx` already has `iso` — pass it down to `<DailyNoteSection iso={iso} />`.

### EntityHeader: honor `isTitleEditable`

Plug `isTitleEditable(entity.structureType)` into `EntityHeader` in `routes/e.$id.tsx`. When false, render the title as a non-editable `<h1>` instead of the editable `Input`. The delete button stays enabled — non-editable title doesn't mean undeletable.

For DailyNote, this means `/e/$id` shows the formatted date as a read-only header. For Note (and any future structure with `title.editable !== false`), behavior is unchanged.

### What the user sees

1. Sidebar Calendar → `/calendar` → today.
2. Daily note section shows the "New" empty state.
3. Click → entity is created, editor mounts focused. Type → content saves with the standard 300 ms debounce.
4. Navigate to a different day → empty state again. Type there → second DailyNote.
5. Click expand on a populated section → `/e/{id}`. Title reads as "April 25, 2026", non-editable. Body editor works as it does for Notes.
6. Refresh anywhere → both DailyNotes persist.

## File changes

```
proto/calcifer/v1/
  entities.proto                 ← PropertyValue.date_value (Timestamp) → date (string), same field 3
gen/ts/calcifer/v1/
  entities_pb.ts                 ← regenerated by `buf generate`
calcifer/src/
  model/
    structures.ts                ← DailyNote: date property + title.editable=false
    store.ts                     ← createDailyNote(iso); dailyNoteByDate(entities, iso)
  components/
    entity/
      EntityRichTextField.tsx    ← NEW: extracted from routes/e.$id.tsx
    calendar/
      DailyNoteSection.tsx       ← rewrite: lazy create + editor mount + delete
      DayView.tsx                ← pass iso to DailyNoteSection
      MiniCalendar.tsx           ← cosmetic class tweaks (cell sizing)
  editors/tiptap/
    TiptapEditor.tsx             ← autoFocus prop → useEditor({ autofocus })
  routes/
    e.$id.tsx                    ← import shared EntityRichTextField; honor isTitleEditable
```

No new deps. One proto change (replace, not add — same field number).

## Out of scope

- **Linking date chips to DailyNotes.** Date chips still don't resolve to entities. Their click still goes through `?date=$iso` (Part 2 behavior). Making a chip's `links[]` point at the DailyNote for its day is a separate piece — needs a story for "what does a chip link to on days that have no DailyNote yet."
- **Real "Date references" backlinks.** The cosmetic example row stays. Real wiring needs `linkSync.ts` to scan dateChip nodes against a `date:${iso}` target (or against a DailyNote, once chip→DailyNote linking lands).
- **Today button.** Sidebar Calendar already covers it.
- **NL parsing** (`"today"`, `"next monday"` → ISO).
- **"..." menu in section header.**
- **Cosmetic "Tags" chip in the section.** Tag attachment is a separate concern; `#tag` in the editor body still works.
- **Pruning empty DailyNotes.** Click-to-create is eager; empty entities can sit. Easy follow-up.
- **Editing a DailyNote's date** (i.e. moving a journal entry to another day). Title and date property are both non-editable in Part 3.

## Verification

`pnpm dev`, then:

1. **Empty state** — Navigate to `/calendar`. Daily note section shows the "New" empty state. No DailyNote exists in localStorage.
2. **Create + persist** — Click the empty state. The editor mounts focused. Type a sentence. Reload — content persists. Inspect localStorage: one DailyNote entity with `title = "April 25, 2026"` (or today's formatted date) and a `date` property whose value is today's ISO.
3. **Day-to-day independence** — Click `›` to advance one day. Empty state again. Type something — a *second* DailyNote is created with the next day's date. Click `‹` back — first day's content reappears.
4. **DateChip → DailyNote** — Insert a date chip in any Note's body, click it, land on the calendar for that date. Click the empty state. Confirm the DailyNote was created for the chip's date.
5. **Expand** — From a populated daily note section, click the expand icon. Lands on `/e/{id}`. Title reads as the formatted date and is non-editable. Body editor works (mentions, slash menu, formatting). `/e/$id`'s delete button still works.
6. **Delete from calendar** — From a populated daily note section, click the trash icon. Confirmation dialog appears; confirm. Section flips back to the empty state; the entity is gone from localStorage.
7. **Note title still editable** — Open any Note's `/e/$id` page. Title is still an editable input. (Confirms `isTitleEditable` only flips for DailyNote.)
8. **No regressions** — Mention insertion, backlinks panel on Notes, sidebar Calendar/Structure links, alt-click DateChip re-edit picker.

## Definition of done

- `PropertyValue.date` is a `string` (`yyyy-MM-dd`); the old `date_value: Timestamp` case is gone; TS types regenerated.
- `DailyNote` has a `date` property of type `date` and a non-editable title; keeps `creatable: false` so it stays out of the New menu.
- `createDailyNote(iso)` and `dailyNoteByDate(entities, iso)` are available.
- `EntityRichTextField` lives in `src/components/entity/` and is imported by both `routes/e.$id.tsx` and `components/calendar/DailyNoteSection.tsx`.
- `DailyNoteSection` lazy-creates a DailyNote on click, mounts the editor, and supports delete from its header.
- `EntityHeader` renders DailyNote titles read-only.
- All verification steps pass; no regressions on existing Note/Tag entity flows.
