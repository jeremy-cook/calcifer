# Phase 6 Part 4 — `referenced_dates` + real date references + DailyNote pruning

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 6.
> Predecessors: [`phase-6-part-1-calendar-layout.md`](./phase-6-part-1-calendar-layout.md) (`5e601c6`), [`phase-6-part-2-datechip-navigation.md`](./phase-6-part-2-datechip-navigation.md) (`527231d`), [`phase-6-part-3-dailynote-wiring.md`](./phase-6-part-3-dailynote-wiring.md) (`617a98f`).

## Context

After Parts 1–3 the calendar surface and DailyNote core flow shipped, but the calendar's "Date references" panel was still a hardcoded placeholder row, and clicking into a calendar day eagerly created a DailyNote that stuck around even if the user typed nothing.

The original Phase 6 sketch closed both gaps via a `DateRef` Structure — a date-as-entity that chips and DailyNotes both link to, leveraging the existing backlinks plumbing. On reflection, that's two jobs in one entity (backlink anchor AND page-for-a-date) and forces entity create/delete churn every time chips appear and disappear in a doc. The chip already navigates to `/calendar?date=$iso`, which IS the page-for-a-date — there's no need for a DateRef entity at all.

**Better shape:** record date references as a field on the source entity. `linkSync` already walks every doc on save to update `Entity.links`; the same pass collects every `dateChip.attrs.date` into a parallel `referenced_dates: string[]` field. The calendar's right column becomes a search — "entities whose `referenced_dates` includes this iso." No entity churn, no get-or-create indirection, no DateRef cleanup.

## Goal

- Every entity carries the set of iso dates its body chips reference, kept in sync on every save with the same shape as `Entity.links`.
- The calendar's "Date references" section lists real source entities for the displayed day (excluding any DailyNote, since the journal is shown above).
- An empty DailyNote is auto-deleted when the user navigates away from its day, so click-to-create doesn't leave junk entities behind.

## Design

### Proto: new repeated field on `Entity`

```proto
// proto/calcifer/v1/entities.proto
message Entity {
  string id = 1;
  string structure_type = 2;
  string title = 3;
  repeated Property properties = 4;
  repeated LinkRef links = 5;
  google.protobuf.Timestamp created_at = 6;
  google.protobuf.Timestamp updated_at = 7;
  repeated string referenced_dates = 8;  // NEW
}
```

Additive. Existing persisted entities decode with an empty array. Regenerate TS via `pnpm proto:gen`.

### linkSync extension

`extractMentions` in `src/model/linkSync.ts` walks the doc for `mention` and `hashtag` nodes today. Rename it to `extractDocReferences` and extend the walk to also collect `dateChip` isos. Split the return:

```ts
interface DocReferences {
  entities: ExtractedMention[]
  dates: string[]
}

export function extractDocReferences(doc: JSONContent): DocReferences { /* ... */ }

export function syncLinksFromDoc(entityId: string, doc: JSONContent): void {
  const { entities: mentions, dates } = extractDocReferences(doc)
  // existing reconcile of Entity.links from `mentions`
  // NEW: reconcile Entity.referenced_dates from `dates` with the same
  //      set-equality short-circuit as setLinks
}
```

Add `setReferencedDates(id, isos[])` to the store mirroring `setLinks` shape exactly — same set-equality short-circuit, same `updatedAt` bump.

### Selector

```ts
export function entitiesByDate(
  entities: Record<string, Entity>,
  iso: string,
): Entity[] {
  return Object.values(entities).filter((e) => e.referencedDates.includes(iso))
}
```

O(N), fits next to `dailyNoteByDate`. For a personal-scale KB this is fine; if it ever isn't, an in-memory `Map<iso, Set<entityId>>` index is a small follow-up.

**Selector gotcha:** consumers must NOT subscribe via `useEntityStore((s) => entitiesByDate(s.entities, iso))` — the filter returns a fresh array on every call, which makes Zustand's `useSyncExternalStore` snapshot identity flap and triggers `Maximum update depth exceeded`. The pattern is to subscribe to `s.entities` (a stable reference unless something changed) and derive via `useMemo`. Same shape `useBacklinks` already uses.

### Calendar wiring

Rewrite `DateReferencesSection`:

```tsx
interface DateReferencesSectionProps { iso: string }

export function DateReferencesSection({ iso }: DateReferencesSectionProps) {
  const entities = useEntityStore((s) => s.entities)
  const filtered = useMemo(
    () => entitiesByDate(entities, iso).filter((e) => e.structureType !== 'DailyNote'),
    [entities, iso],
  )
  if (filtered.length === 0) return <EmptyState />
  return <List entities={filtered} />
}
```

DailyNote sources are filtered out — the day's journal is rendered by `DailyNoteSection` above; "Date references" means *other things that referenced this day*. Each row: structure icon + title, click → `/e/$id`. Inline-rendered (no shared row component yet; if backlinks rows ever factor out cleanly, dedupe later).

`DayView` already has `iso` — pass it down.

### DailyNote pruning

On navigating away from a calendar day (or unmount), if the DailyNote's content doc is empty, delete it. Nothing else points at DailyNotes (chips reference isos, not DailyNote ids), so pruning can't orphan anything.

```tsx
useEffect(() => () => {
  const state = useEntityStore.getState()
  const existing = dailyNoteByDate(state.entities, iso)
  if (!existing) return
  const ref = contentRichTextRef(existing)
  const doc = ref ? useRichTextStore.getState().getRichText(ref) : undefined
  if (isRichTextEmpty(doc?.doc)) {
    state.deleteEntity(existing.id)
  }
}, [iso])
```

Cleanup reads fresh state from the store (not closure capture) so it sees the latest entity and doc on the way out.

`isRichTextEmpty` is a small helper in `src/model/richtext.ts`: parses the doc string and treats a missing/empty doc, or a doc whose only top-level child is an empty paragraph, as empty.

**Edge case:** user types one char, deletes it, navigates away → pruned. That's intent (they discarded). Edge case in the other direction: types within the 300ms richtext debounce window and navigates away → cleanup reads stale empty store → prunes a non-empty note. Live with this for now; the fix would be to hoist TipTap's `editor.isEmpty` up via a callback ref, which is more plumbing than the rare race deserves at the moment.

## File changes

```
proto/calcifer/v1/
  entities.proto                  ← add referenced_dates field
calcifer/gen/ts/calcifer/v1/
  entities_pb.ts                  ← regenerated
calcifer/src/
  model/
    store.ts                      ← setReferencedDates(id, isos); entitiesByDate(entities, iso)
    linkSync.ts                   ← extractDocReferences; reconcile referenced_dates
    richtext.ts                   ← isRichTextEmpty(doc)
  components/
    calendar/
      DateReferencesSection.tsx   ← rewire to real data via useMemo
      DayView.tsx                 ← pass iso
      DailyNoteSection.tsx        ← prune on iso change
```

No new deps. One additive proto change.

## Out of scope

- `DateRef` Structure entirely. Dates are not entities in this design.
- NL date parsing. Slash-keyword shortcuts (`/today`, `/tomorrow`, `/yesterday`) are a possible Part 5; free-form parsing is not on the roadmap.
- Calendar markers for days that have a journal or chip references.
- Bulk migration. Chips written before Part 4 won't appear in `referenced_dates` until their containing doc is re-saved. Re-saving naturally repairs; no migration loop.
- "Doc was never edited" pruning. Empty-on-leave is good enough.
- Pruning DailyNotes on app load or via a sweep. Effect-cleanup on iso change is sufficient.
- `editor.isEmpty` hoisting for the pruning race. Acceptable risk for now.

## Verification

`pnpm dev`, then:

1. **Chip writes referenced_dates** — Insert a date chip in a Note. Wait for debounce. Inspect localStorage: the source entity's `referencedDates` array contains the chip's iso.
2. **Multiple chips, one entity** — Add two chips for different days. Array has both isos. Delete one chip, save — array drops it.
3. **Calendar references panel** — Navigate to `/calendar?date=$chipIso`. Date references section lists the source Note. Click → lands on `/e/$id` for that note.
4. **DailyNote excluded** — On a day with both a DailyNote and a chip pointing at it, the references section shows the chip-source note but excludes the DailyNote.
5. **Empty references state** — A day with no chips shows "No date references."
6. **DailyNote pruning (empty)** — Click into a calendar day, do nothing, click `›`. The created DailyNote is gone from localStorage.
7. **DailyNote pruning (deleted text)** — Click in, type a sentence, wait 300ms, delete all the text, wait 300ms, navigate away → DailyNote pruned.
8. **DailyNote persists** — Click in, type, wait 300ms, navigate away → DailyNote persists. Coming back shows the content.
9. **No regressions** — Mention insertion, backlinks panel on Notes, sidebar navigation, alt-click DateChip re-edit picker, expand from calendar.

## Definition of done

- `Entity.referenced_dates` carries every iso a chip references in the entity's body; reconciled on every save with the same shortcut as `setLinks`.
- `entitiesByDate(entities, iso)` returns the right set; calendar references section uses it; DailyNote sources are excluded.
- Empty DailyNotes auto-prune on navigation away from their calendar day.
- All verification steps pass; no regressions on existing Note/Tag entity flows or the calendar surface from Parts 1–3.
