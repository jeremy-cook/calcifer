# Phase 4 — Mentions Wired to the Entity Store

## Context

Phase 3 shipped a proto-backed Zustand entity store and an entity page (`/e/:id`) mounting `TiptapEditor`. The editor's `@` and `#` mention extensions currently use hardcoded static arrays. Phase 4 wires them to live entity data, adds extended node attrs per spec.md §4.1, introduces slash-notation type narrowing (`@note/query`), derives `LinkRef`s from document content on save, and enables click-to-navigate on chips.

**Scope:** Suggestion UX + link derivation only. Entity creation from mention menus is deferred.

---

## Key Design Decisions

### Naming

- `structure_id` → `structure_type` (proto field) — it's a type tag (e.g. `'Note'`), not a UUID. `id` is misleading.
- `RootTag` → `Tag` (structure key and id) — drop the Capacities-ism.

### Two extensions, shared base, unified factory

Keep the existing pattern — **two** `@tiptap/extension-mention` instances (`EntityMention` and `HashtagMention`) — because it's idiomatic TipTap: each extension owns one `suggestion` config. Dedupe the custom attrs / NodeView via a **shared base** (`HashtagMention = EntityMention.extend({ name: 'hashtag' })`), and dedupe the suggestion logic via a **single factory** `makeSuggestion({ char, structureFilter? })` invoked twice at the editor-configure site:

- `@` → `makeSuggestion({ char: '@' })` — searches all entities, slash-notation narrows (`@note/jane`, `@tag/design`)
- `#` → `makeSuggestion({ char: '#', structureFilter: 'Tag' })` — Tags only, no narrowing

Two node types in the schema (`mention`, `hashtag`); `data-type` attributes and CSS classes are differentiated via each extension's `HTMLAttributes`.

### Links derived from content, not written at insert-time

The previous plan wrote a `LinkRef` in the suggestion's `command` callback. That's wrong: if the user deletes a mention, the link leaks. Undo/redo doesn't work. The editor doesn't need to know which entity owns the doc.

Instead: **mentions live in the doc JSON**. Links are derived by walking the JSON tree on save. `EntityRichTextField.onUpdate` diffs the derived set against `entity.links[]` and calls `setLinks`. `TiptapEditor` stays decoupled from the entity model.

### Click routing depends on structure

Dispatch via `switch (structureType)`:
- `case 'Tag'` → `/tag/$id` (dedicated tag view; page can be empty for now — Phase 5 will show entities linking to this tag)
- `default` → `/e/$id` (entity page)

### Suggestion popup — reuse slash-menu machinery

`slashSuggestion.ts`, `mentionSuggestion.ts`, `hashtagSuggestion.ts` each duplicate the entire `ReactRenderer` + Floating UI + onKeyDown lifecycle. Extract to a single `createSuggestionPopup(MenuComponent)` helper used by all three. Keep `ReactRenderer` + Floating UI (it's the right pattern for TipTap — the popup anchors to cursor position, not a trigger element; shadcn's `Popover` doesn't apply).

### Structure colors

Add a `color` CSS token per structure in `structures.ts` (reusing `var(--chart-N)` already in `TiptapEditor.css`). Used as a colored dot indicator in the suggestion menu and as the chip accent color.

---

## Implementation Plan

### Part A — Renames

#### A.1 Proto: `structure_id` → `structure_type`

**File:** `proto/calcifer/v1/entities.proto`

Rename the field in `EntityRef` and `Entity`. Regenerate TS bindings (`buf generate`). The regenerated `Entity` / `EntityRef` will expose `structureType: string` in TS.

#### A.2 Code: `structureId` → `structureType`

Sweep usage across: `src/model/*.ts`, `src/routes/*.tsx`, `src/layouts/**/*.tsx`, `src/editors/**/*.ts(x)`. Includes type params, comparisons, and Entity message creation.

#### A.3 Structure: `RootTag` → `Tag`

**File:** `src/model/structures.ts`
- Rename key `RootTag` → `Tag` and `id: 'RootTag'` → `id: 'Tag'`
- Update `StructureId` type consumers (`CreatableStructureId`, comparisons elsewhere)

---

### Part B — Core Model Changes

#### B.1 Add `color` to `StructureMeta`

**File:** `src/model/structures.ts`

Add `color: string` to `StructureMeta`:
```typescript
Note:      { ..., color: 'var(--chart-1)' }
Tag:       { ..., color: 'var(--chart-2)' }
DailyNote: { ..., color: 'var(--chart-3)' }
```

#### B.2 Store: `setLinks(entityId, links)`

**File:** `src/model/store.ts`

Add:
```typescript
setLinks: (entityId: string, links: LinkRef[]) => void
```

Clones the entity with the new `links` array and bumped `updatedAt`. No-op when entity missing. Replaces the full set — no incremental add/remove API, since we derive the full set from doc content.

Import `LinkRefSchema`, `EntityRefSchema` from `@calcifer/proto/calcifer/v1/entities_pb`.

#### B.3 Link derivation utility

**Create:** `src/model/linkSync.ts`

```typescript
// Walks TipTap JSON; returns { id, structureType } for each mention node
export function extractMentions(doc: JSONContent): Array<{ id: string; structureType: string }>

// Diffs extracted mentions against entity.links[] and writes back via setLinks
export function syncLinksFromDoc(entityId: string, doc: JSONContent): void
```

`syncLinksFromDoc` builds `LinkRef` messages (with fresh UUIDs and `createdAt` timestamps) for any newly-mentioned entities not already linked, drops `LinkRef`s whose target no longer appears in the doc, preserves existing `LinkRef.id` and `createdAt` for continuing mentions (match by `target.id`). Final set is committed via `setLinks`.

---

### Part C — Mention Extensions (shared base)

#### C.1 `EntityMention` + `HashtagMention`

**Create:** `src/editors/tiptap/extensions/entityMention.ts`

Extend `@tiptap/extension-mention`'s `Mention` once with the custom attrs and NodeView; derive the hashtag variant by renaming:

```typescript
import { Mention } from '@tiptap/extension-mention'
import { ReactNodeViewRenderer } from '@tiptap/react'
import { MentionNodeView } from './MentionNodeView'

export const EntityMention = Mention.extend({
  addAttributes() {
    return {
      ...this.parent?.(),    // keeps default id + label
      structureType: { default: null },
      char:          { default: '@' },
    }
  },
  addNodeView() {
    return ReactNodeViewRenderer(MentionNodeView)
  },
})

export const HashtagMention = EntityMention.extend({ name: 'hashtag' })
```

Configured at the editor-setup site (see F.1) — each gets its own `HTMLAttributes` and `suggestion`.

#### C.2 `MentionNodeView`

**Create:** `src/editors/tiptap/extensions/MentionNodeView.tsx`

Renders a `NodeViewWrapper as="span"` with the `char + label`. Click dispatches via `switch`:

```typescript
function handleClick() {
  const { id, structureType } = node.attrs
  switch (structureType) {
    case 'Tag':
      router.navigate({ to: '/tag/$id', params: { id } })
      break
    default:
      router.navigate({ to: '/e/$id', params: { id } })
  }
}
```

Uses the `router` singleton from `src/router.tsx`.

---

### Part D — Shared Suggestion Popup Infrastructure

#### D.1 `createSuggestionPopup` helper

**Create:** `src/editors/tiptap/components/suggestionPopup.ts`

Exports one helper:
```typescript
export function createSuggestionPopup<T>(MenuComponent: ComponentType<SuggestionMenuProps<T>>):
  SuggestionOptions<T>['render']
```

Encapsulates the `ReactRenderer` + Floating UI lifecycle currently duplicated in `slashSuggestion.ts`, `mentionSuggestion.ts`, and `hashtagSuggestion.ts`:
- `onStart`: mount ReactRenderer, append to body, position
- `onUpdate`: repopulate props, reposition
- `onKeyDown`: handle Escape, delegate navigation keys to menu's imperative handle
- `onExit`: cleanup

Includes the `updatePosition` helper (Floating UI `computePosition` + `posToDOMRect`).

#### D.2 Refactor `slashSuggestion.ts`

Replace its inline `render` / `updatePosition` with:
```typescript
render: createSuggestionPopup(SlashCommandMenu)
```

#### D.3 Delete `mentionSuggestion.ts` and `hashtagSuggestion.ts`

Their logic merges into `makeSuggestion.ts` (next part).

---

### Part E — Mention Suggestion Factory

#### E.1 `EntitySuggestionItem` type

**Create:** `src/editors/tiptap/components/mention/types.ts`

```typescript
export type EntitySuggestionItem = {
  id: string            // entity UUID
  label: string         // entity title
  structureType: string // 'Note' | 'Tag' | 'DailyNote'
  color: string         // from STRUCTURES[structureType].color
}
```

#### E.2 Slash-notation parser

**Create:** `src/editors/tiptap/components/mention/parseQuery.ts`

```typescript
// parseQuery('note/alice') → { structureType: 'Note', term: 'alice' }
// parseQuery('alice')      → { structureType: null,   term: 'alice' }
export function parseQuery(query: string): { structureType: StructureId | null; term: string }
```

Splits on first `/`. Matches prefix case-insensitively against `STRUCTURE_LIST` by `name.toLowerCase()` (so `note` → `'Note'`, `tag` → `'Tag'`). Unknown prefix → treat whole query as term.

#### E.3 Unified `makeSuggestion` factory

**Create:** `src/editors/tiptap/components/mention/makeSuggestion.ts`

```typescript
interface SuggestionConfig {
  char: '@' | '#'
  structureFilter?: StructureId  // when set, locks filter and skips slash parsing
}

export function makeSuggestion(config: SuggestionConfig):
  Omit<SuggestionOptions<EntitySuggestionItem>, 'editor'>
```

- `char: config.char`
- `items({ query })`:
  - If `config.structureFilter` set: filter is fixed, `term = query`
  - Else: `{ structureType, term } = parseQuery(query)`
  - Read `useEntityStore.getState().entities`, filter by `structureType` (if set) and `title.toLowerCase().includes(term)`
  - Map to `EntitySuggestionItem[]` with `color` from `STRUCTURES`; cap at 8
- `command({ editor, range, props })`:
  - `editor.chain().focus().deleteRange(range).insertContent({ type: 'mention', attrs: { id, label, structureType, char: config.char } }).run()`
  - **No link-writing here** — derived on save
- `render: createSuggestionPopup(MentionMenu)`

#### E.4 `MentionMenu` component

**Create:** `src/editors/tiptap/components/mention/MentionMenu.tsx`

Thin cmdk-based menu. Reuses `.slash-menu` CSS classes for visual consistency with slash menu. Each row:
```tsx
<Command.Item ...>
  <span className="size-2 rounded-full shrink-0" style={{ background: item.color }} />
  <span>{item.label}</span>
</Command.Item>
```

Exposes the same imperative `onKeyDown` interface the popup helper expects.

---

### Part F — Wire It Up

#### F.1 `TiptapEditor`

**File:** `src/editors/tiptap/TiptapEditor.tsx`

Replace lines 112–119 with the two extended variants, each configured at the call site:

```typescript
// BEFORE
Mention.configure({ HTMLAttributes: { class: 'mention mention--user' }, suggestion: mentionSuggestion }),
Mention.extend({ name: 'hashtag' }).configure({ HTMLAttributes: { class: 'mention mention--tag' }, suggestion: hashtagSuggestion }),
```

```typescript
// AFTER
EntityMention.configure({
  HTMLAttributes: { class: 'mention mention--user' },
  suggestion: makeSuggestion({ char: '@' }),
}),
HashtagMention.configure({
  HTMLAttributes: { class: 'mention mention--tag' },
  suggestion: makeSuggestion({ char: '#', structureFilter: 'Tag' }),
}),
```

Remove `import { Mention } from '@tiptap/extension-mention'` (replaced). Add imports for `EntityMention`, `HashtagMention`, `makeSuggestion`.

**No `entityId` prop.** The editor stays content-focused and reusable.

#### F.2 `EntityRichTextField`

**File:** `src/routes/e.$id.tsx`

The existing `onUpdate` callback receives the TipTap JSON. Extend it:

```typescript
const handleUpdate = useCallback((json: JSONContent) => {
  if (timerRef.current) clearTimeout(timerRef.current)
  timerRef.current = setTimeout(() => {
    putRichText(propertyRef, JSON.stringify(json))
    syncLinksFromDoc(propertyRef.entityId, json)
  }, RICHTEXT_DEBOUNCE_MS)
}, [propertyRef, putRichText])
```

(This also fixes the existing type mismatch where `handleUpdate` is typed as `(next: string)` but `TiptapEditor.onUpdate` passes `JSONContent`.)

#### F.3 New route: `/tag/$id`

**Create:** `src/routes/tag.$id.tsx`

Dedicated tag view. For Phase 4 the page can render a placeholder (title from the Tag entity, "No content yet"). Phase 5 will populate it with entities that link to the tag. The route exists so `#tag` click-nav round-trips cleanly.

---

## Critical Files

| File | Change |
|---|---|
| `proto/calcifer/v1/entities.proto` | Rename `structure_id` → `structure_type` |
| `src/model/structures.ts` | Rename `RootTag` → `Tag`, add `color` field |
| `src/model/store.ts` | Add `setLinks`; update `structureType` references |
| `src/model/linkSync.ts` | **New** — `extractMentions` + `syncLinksFromDoc` |
| `src/editors/tiptap/TiptapEditor.tsx` | Replace base `Mention` with `EntityMention` / `HashtagMention`; no new prop |
| `src/editors/tiptap/extensions/entityMention.ts` | **New** — `EntityMention` (base) + `HashtagMention` (rename-extend) |
| `src/editors/tiptap/extensions/MentionNodeView.tsx` | **New** — click dispatch by structure type |
| `src/editors/tiptap/components/suggestionPopup.ts` | **New** — shared popup lifecycle helper |
| `src/editors/tiptap/components/mention/makeSuggestion.ts` | **New** — unified factory for `@` and `#` |
| `src/editors/tiptap/components/mention/MentionMenu.tsx` | **New** — cmdk menu with colored dot |
| `src/editors/tiptap/components/mention/types.ts` | **New** — `EntitySuggestionItem` |
| `src/editors/tiptap/components/mention/parseQuery.ts` | **New** — slash-notation parser |
| `src/editors/tiptap/components/mention/mentionSuggestion.ts` | **Delete** |
| `src/editors/tiptap/components/mention/hashtagSuggestion.ts` | **Delete** |
| `src/editors/tiptap/components/mention/SuggestionMenu.tsx` | **Delete** (superseded by `MentionMenu`) |
| `src/editors/tiptap/components/slash-menu/slashSuggestion.ts` | Refactor to use `createSuggestionPopup` |
| `src/routes/e.$id.tsx` | `onUpdate` calls `syncLinksFromDoc` |
| `src/routes/tag.$id.tsx` | **New** — placeholder tag view (filled in Phase 5) |
| Codebase-wide | Sweep `structureId` → `structureType`, `'RootTag'` → `'Tag'` |

**Reuse:**
- `router` at `src/router.tsx:4`
- `useEntityStore.getState()` — outside-React access
- `STRUCTURES`, `STRUCTURE_LIST` from `src/model/structures.ts`
- `create`, `timestampNow` from `@bufbuild/protobuf` / `@bufbuild/protobuf/wkt`
- `LinkRefSchema`, `EntityRefSchema` from the generated proto
- `.mention`, `.mention--user`, `.mention--tag`, `.slash-menu*` CSS in `TiptapEditor.css`
- `SlashCommandMenu` / `slashSuggestion` — refactored to share popup lifecycle

---

## Verification

1. `pnpm dev` in `calcifer/`
2. Open a Note → type `@` → menu shows live entities with colored dots per structure
3. `@note/` narrows to Notes; `@tag/` narrows to Tags
4. `#` shows Tags only
5. Select an entity → mention chip inserted
6. Reload → mentions persist in the doc JSON
7. Open entity in devtools / localStorage → source entity's `links[]` contains a `LinkRef` for each mention (derived, debounced)
8. Delete a mention in the editor, wait for debounce → that `LinkRef` is gone from `links[]`
9. Click a `@note/...` chip → navigates to `/e/:id`
10. Click a `#tag` chip → navigates to `/tag/<tagId>` (page can be the placeholder — the route is reached)
