# Phase 5 — Backlinks

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 5.

## Goal

Show, on every entity page, the list of other entities that link to it — so the user can navigate the inverse of the mention graph. Pure derivation from the existing `Entity.links[]` outgoing edges; no new persisted state, no new proto fields.

## Behavior

- The panel is **collapsible, collapsed by default** on every navigation. The header always shows total backlink count: e.g. `Backlinks (5)`. Click the header to expand; rows are mounted lazily on first expand.
- **Always rendered**, even when count = 0 (header shows `Backlinks (0)`). Keeps the affordance discoverable.
- **Keyboard shortcut: `Cmd+Shift+B`** (Ctrl+Shift+B on non-Mac) toggles the panel without reaching for the mouse.
- A row per **source entity** (deduped — multiple mentions of the target by the same source still produce one row).
- Each row shows: source Structure icon (Structure color) + source entity title. **No snippet.** Mentions are visually distinct in the editor; the user clicks through if they want context.
- **Two view modes**, toggled by a small icon button in the panel header:
  - **Grouped** (default) — sectioned by source Structure, ordered by `STRUCTURE_LIST` index; within each section sorted by most-recent `LinkRef.created_at` desc.
  - **Flat** — all backlinks in a single list sorted by most-recent `LinkRef.created_at` desc, regardless of Structure. Each row still shows the Structure icon so the type is visible.
- Click a row → navigate to `/e/$id` of the source entity. **No scroll-to-mention, no highlight.**
- Self-links excluded.

## File changes

```
calcifer/src/
  model/
    backlinks.ts                   ← NEW: derived selector + types
  components/
    backlinks/
      BacklinksPanel.tsx           ← NEW: panel UI (group, header, rows, shortcut)
    ui/
      collapsible.tsx              ← NEW (via shadcn add)
  routes/
    e.$id.tsx                      ← mount BacklinksPanel below properties
    tag.$id.tsx                    ← DELETE (consolidate into /e/$id)
  editors/tiptap/extensions/
    MentionNodeView.tsx            ← drop /tag/$id branch; always /e/$id
  router.tsx / routeTree.gen.ts    ← regenerate after deleting tag route
```

## Engineering detail

### 1. Derived selector (`src/model/backlinks.ts`)

```ts
// Lightweight projection — only the fields the panel actually renders.
// Avoids leaking the full Entity (with its richtext refs, properties, etc.) into the UI layer.
export interface Backlink {
  entityId: string
  structureType: string
  title: string
  // Most recent created_at across all LinkRefs from this source pointing at the target.
  mostRecentAt: Date
}

export function useBacklinks(targetEntityId: string): Backlink[]
```

Implementation:
- Read `entities` from `useEntityStore` via a selector.
- Iterate every entity once; for each, scan `links[]` and find entries where `link.target?.id === targetEntityId`. Collect the max `created_at` across matches.
- Skip self-links (`entity.id === targetEntityId`).
- Project to `Backlink` (just `entityId`, `structureType`, `title`, `mostRecentAt`).
- Return sorted by `(structureType, -mostRecentAt)` — the grouped view groups consecutive entries; the flat view re-sorts client-side by `mostRecentAt` only.

Perf note: O(n × avg_links) per render. Fine for hundreds-to-low-thousands of entities. If it becomes a bottleneck, build an inverted index in the store; not worth doing speculatively.

### 2. `BacklinksPanel` (`src/components/backlinks/BacklinksPanel.tsx`)

```ts
interface BacklinksPanelProps {
  entityId: string
}
```

- Calls `useBacklinks(entityId)`. Open/closed lives in `useState` (default `false`) so the keyboard shortcut can flip it; view-mode toggle is the same shape.
- Always renders — no count = 0 early return; header shows `Backlinks (0)` and the panel is still expandable (just shows nothing inside).
- Renders a shadcn `<Collapsible open={open} onOpenChange={setOpen}>`:
  - `<CollapsibleTrigger>` — header showing `Backlinks ({total})` with a chevron icon that rotates on open.
  - To the right of the count: a small icon button toggling view mode (`grouped` ↔ `flat`). Use icons like Phosphor `ListBulletsIcon` (flat) ↔ `RowsIcon` or `StackIcon` (grouped). Stop click propagation so toggling view doesn't open/close the panel.
  - `<CollapsibleContent>` — list. Lazy-render: gate children on `open` so we don't materialize rows until first expand.
- **Grouped mode**: walk the selector output (already `(structureType, -mostRecentAt)` sorted) and emit a section header each time `structureType` changes. Section header: `{Structure plural} ({groupCount})`. Rows: Structure icon (Structure color) + title, as `<Link to="/e/$id" params={{ id: entityId }}>`.
- **Flat mode**: re-sort the selector output by `mostRecentAt` desc only. Render each row identically to grouped mode (icon + title), no section headers.

Visual placement: full-width strip below the editor body. Top border to separate from body content. Match the existing entity-page padding (`px-12 / px-16` per current layout).

Component style (per CLAUDE.md):
- Named `BacklinksPanelProps` interface.
- Constants (icon lookup, padding class, etc.) above the `return` inside the function body.
- If `BacklinkSection` grows past ~5 lines, extract as a sub-component with its own props interface.

### 2b. Keyboard shortcut

Add a `useEffect` inside `BacklinksPanel` that registers a window `keydown` listener while the panel is mounted:

```ts
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === 'B' || e.key === 'b')) {
      e.preventDefault()
      setOpen((o) => !o)
    }
  }
  window.addEventListener('keydown', onKey)
  return () => window.removeEventListener('keydown', onKey)
}, [])
```

- Mac uses `metaKey`; Win/Linux uses `ctrlKey` — handle both.
- Check both `'B'` and `'b'` because the shift modifier reports `'B'` on most browsers.
- TipTap's default Bold binding is `Cmd+B` (no Shift) so this won't collide with editor formatting.
- The listener lives inside the panel because there's no panel to toggle when no entity page is mounted; tying lifecycle to the component avoids stale handlers.

If multiple panels ever render on the same page (not currently possible), scope the shortcut differently. For v1, one entity page = one panel = one listener.

### 2a. shadcn dependency

`Collapsible` isn't installed yet. Add it before implementing the panel:

```bash
pnpm dlx shadcn@latest add collapsible
```

This drops `src/components/ui/collapsible.tsx`.

### 3. Mount on entity page (`src/routes/e.$id.tsx`)

Add below `<EntityProperties>`:

```tsx
<div className="border-t border-border px-12 py-6">
  <BacklinksPanel entityId={entity.id} />
</div>
```

Works uniformly for body-having entities (Note, DailyNote) and body-less entities (Tag, future DateRef). For body-less entities, the properties area renders nothing and the backlinks panel is the page content beneath the title.

### 4. Drop `/tag/$id`

- Delete `src/routes/tag.$id.tsx`.
- In `src/editors/tiptap/extensions/MentionNodeView.tsx`, remove the `/tag/$id` branch — always navigate to `/e/$id` regardless of `structureType`.
- Re-run codegen for `routeTree.gen.ts` (TanStack Router regenerates on dev-server restart, or run the codegen script if present).

### 5. Sort details

Selector returns:
```
[ ...sourceEntities sorted by (structureType lex, mostRecentAt desc) ]
```

Component groups consecutive entries with the same `structureType`. This gives the visual grouping without a second pass.

If later the user wants a flat "all backlinks by date" view, add a toggle in the panel header — no plumbing changes needed.

### 6. Edge cases

- **Stale links** (target was deleted but LinkRef still exists in some source): not an issue for this panel — we look up the source entity from the store, and rows only render for sources whose entity object is present. The orphaned LinkRef is harmless data; `linkSync` already cleans up the source's links on its next edit.
- **Self-link**: excluded in the selector.
- **Multiple mentions of the same target by the same source**: deduped to one row; sort uses the most recent `created_at`.
- **Renamed source**: title comes live from the source entity, so backlinks reflect renames automatically.

## Verification

End-to-end browser walkthrough:

1. Create Note A, then Note B with body containing `@A`.
2. Open Note A → "Notes (1)" group with one row "Note B". Click → navigates to Note B.
3. In Note B, mention `@A` two more times. Reload Note A → still one row for Note B (deduped).
4. Edit Note B to remove all `@A` mentions and wait for the link-sync debounce. Reload Note A → "No backlinks".
5. Tag a Note with `#mytag`, then open the Tag at `/e/$tagId` → backlinks shows the tagging Note.
6. Confirm the old `/tag/$id` URL no longer resolves and clicking a `#tag` chip navigates to `/e/$id`.

## Out of scope

- **Highlighting / scroll-to-mention** on click. Visually distinct mention chips are enough.
- **Snippet text** of the mention's surrounding paragraph.
- **Inverted-index store** for O(1) backlinks lookup. Premature for current data sizes.
- **Per-mention `linkId` attribute** on mention nodes. Today the node carries `attrs.id = targetEntityId` only; we don't need per-instance ids for this feature.

## Definition of done

- Every entity page shows a backlinks panel beneath its body.
- Panel reflects mention edits within the link-sync debounce window.
- `/tag/$id` is gone; tag chips and tag pages route through `/e/$id`.
- Phase 5 flips to ✅ in `app-plan.md` and the feature matrix.
