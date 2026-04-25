# Editor Internals — Reference

> **This is engineering reference, not the source of truth.**
> - Application-level decisions (Structures, mention semantics, `mentionable` rules, `DateRef` vs `DailyNote` split, etc.) live in [`../app-plan.md`](../app-plan.md).
> - Editor feature parity tracking lives in [`../editor-comparison-plan.md`](../editor-comparison-plan.md).
> - Phase-4 mention engineering work lives in [`./phase-4-mentions.md`](./phase-4-mentions.md).
>
> This document is the deep-dive research into Capacities, BlockNote, and TipTap that informed the editor architecture (regex shapes, popup CSS, drag-handle alignment math, etc.). Several of its original proposals have been **superseded** by app-plan decisions:
> - `RootTag` → `Tag` (no `Root*` prefix; Calcifer has no user-defined Structures)
> - `UtilDate` → `DateRef` (kept distinct from `DailyNote` — see app-plan Data Model)
> - `structureId` field name → `structure_type` in proto (`structureType` in TS)
> - `@` does *both* reuse and create-on-miss (via tail item). `/Structure/Entity/` create syntax is **dropped** — `/` is the slash menu only.
> - `#` likewise gets create-on-miss for new Tags.
> - `mentionable` is a per-Structure flag (default `true`) — the equivalent of Capacities' single `allowDailyNoteLinking` flag, generalized.
> - Paste/import parity for Capacities-format text is **dropped** for now.
>
> Treat this file as background + CSS/regex reference. For current behavior, follow the links above.

Reference implementations analyzed:
- **Capacities** (`~/Capacities/electron-dist/shared-logic/util/`) — production app with `@` mentions and tags
- **BlockNote** (`~/Github/BlockNote/packages/core/src/extensions/`) — reference for slash menu and drag handle internals
- **TipTap** (`~/Github/tiptap/packages/`) — target library

---

## 0. Research Summary (How This Spec Was Built)

This spec was produced by deep-reading all three codebases. Key decisions and findings from that research:

### What we're building
A TipTap-based rich text editor with:
- `@` entity mentions and `#` tag mentions (real-time suggestion popups)
- `/` slash command menu for block insertion
- Drag handle for block reordering, visually aligned with block text

### Why not BlockNote
BlockNote is ~60–70% custom logic on top of TipTap. It's not a thin wrapper — it has its own schema, block model, block-level operations, and full UI layer. Using it for a single editor brings significant overhead. TipTap's first-party extensions (`extension-drag-handle`, `extension-dropcursor`) and the `suggestion` package cover our needs directly.

### Capacities analysis — what we learned and what we dropped
Capacities resolves `@` mentions via **batch regex scanning at import time** (`importBackbone.js`), not real-time popups. Key findings:

- `[[wikilink]]` and `@mention` are **functionally identical** — both produce `context: 'reuse'` and resolve to the same `LinkToken`. `[[]]` was dropped from this spec; `@` covers the same use case.
- `+Structure/Entity/` and `/Structure/Entity/` both mean "create new entity". `+` was dropped; `/` is kept.
- Entity references store two IDs: `entityId` + `toStructureId`. TipTap's default mention only stores a flat `id` — this spec extends the schema.
- The `NOTE/entityId` format is used internally for round-trip safety (already-resolved links survive re-processing).
- `UtilDate` is a special structure type where the entity title is a natural-language date string.

### BlockNote drag handle vs TipTap `extension-drag-handle`
Both use floating-ui with `placement: 'left-start'`. The **alignment difference** is entirely in setup:
- BlockNote: editor has `padding-inline: 54px`, reference rect X is pinned to the editor block-group left edge, handle height is CSS-matched per block type via data attributes.
- TipTap default: zero floating-ui middleware, no editor padding, handle overlaps or floats incorrectly.
- Fix: add editor padding, use `getReferencedVirtualElement` to pin X, set `offset` middleware, match handle height via `onNodeChange`.

### BlockNote slash menu vs `@tiptap/suggestion`
BlockNote's slash menu uses its own ProseMirror plugin (`handleTextInput`), but it's architecturally identical to `@tiptap/suggestion`. There's no need to replicate it — just configure `suggestion` with `/` as the trigger. The popup UI (container, group labels, items with icon/title/subtitle/badge, floating-ui positioning, keyboard nav, auto-scroll) is the same shared component as the mention popup, just with different item rendering.

---

## 1. Feature Overview

| Feature | Capacities | BlockNote | TipTap (built-in) |
|---------|-----------|-----------|-------------------|
| `@mention` (link existing entity) | Yes — import-time | Yes — real-time | Yes — real-time |
| `/mention` (create + link entity) | Yes — import-time | No | No |
| `#tag` | Yes — import-time | No | No (trivial via Suggestion) |
| Real-time suggestion popup | No | Yes | Yes |
| Multi-level entity refs (structureId + entityId) | Yes | No | No |

> **Note on `[[wikilink]]`:** Capacities supports `[[EntityTitle]]` syntax but it is semantically identical to `@mention` — both resolve to the same `LinkToken` with `context: 'reuse'`. There is no behavioral difference. `[[]]` is **not being implemented** — `@` covers the same use case with less complexity.

> **Note on `+prefix`:** Capacities supports `+Structure/Entity/` to create new entities on import. This is **dropped in favour of `/Structure/Entity/`**, which Capacities also supports for the same purpose.

---

## 2. How Capacities `@` Mention Works (Deep Dive)

### 2.1 The Full Pipeline

```
Raw text/markdown
      ↓
  mdast parser (mdast-util-from-markdown + GFM plugins)
      ↓
  AST traversal (traverseASTRecursive)
      ↓
  paragraph node → paragraphToTokens()
      ↓  [for each 'text' AST node]
  linkEntitiesInText()    ← importBackbone.js
      ↓  [loops while matches exist]
  matchEntityLink()       ← regex scan of raw text
      ↓  [per match]
  matchStructure()        ← structureTitle → structureId
  findEntity()            ← (title, structureIds) → component
  generateEntity()        ← if context='create' and not found
      ↓  [if resolved]
  generateInternalEntityLink()   → "[label](NOTE/entityId)" markdown
      ↓
  mdToBlocksBackbone()    ← re-parse the replaced text
      ↓
  link node → generateLinkNode()
      ↓
  LinkToken { entity: { id, link: { data: { toStructureId } } } }
```

### 2.2 `matchEntityLink()` — Pattern Detection

`importBackbone.js` lines 56–112. Runs two regexes in priority order:

**Regex 1** (mention-style): `matchEntityLink1Regex = /(?:(\+|\/|@))([^\s\/]+)(?:\/)([^\/]+)(?:\/)/gm`

| Prefix | Context | Syntax |
|--------|---------|--------|
| `@` | `'reuse'` — link existing entity only | `@StructureTitle/EntityTitle/` |
| `/` | `'create'` — create if not found | `/StructureTitle/EntityTitle/` |
| `+` | `'create'` — create if not found | `+StructureTitle/EntityTitle/` |

**Regex 2** (wikilink-style, fallback): `matchEntityLink2Regex = /\[\[([^\]\[]+)\]\]/gm`

Always `context: 'reuse'`. Supports `[[EntityTitle]]` or `[[StructureTitle/EntityTitle]]`. Functionally identical to `@reuse` — this is why `[[]]` is not being implemented.

Return value:
```ts
{
  context: 'reuse' | 'create',
  structureTitle: string | undefined,  // e.g. 'Person', 'Note'
  entityTitle: string,                 // e.g. 'Alice'
  matchObject: { text, offset, length }
}
```

### 2.3 `linkEntitiesInText()` — Resolution Loop

`importBackbone.js` lines 192–268. Called per text node during AST traversal.

```
while (entityMatch = matchEntityLink(text)):

  1. Special case: structureTitle === 'NOTE'
       → loadAndGetC(entityTitle)  // load directly by ID
       → skip normal lookup

  2. Resolve structureTitle → structureId
       → matchStructure(structureTitle)
       → if not found and structureTitle provided: skip match

  3. findEntity(entityTitle, { filterStructureIds, excludeIds })
       → searches existing entities by title

  4. If not found AND context='create' AND structureId resolved:
       → generateEntity(structure, ...) creates a new entity
       → sets component.properties.title = entityTitle

  5. Special case: structureId === 'UtilDate'
       → parseNaturalLanguageDate(entityTitle)
       → creates a date entity with the parsed date value

  6. If component resolved:
       → text = text.slice(0, offset)
               + "[entityTitle](NOTE/entityId)"
               + text.slice(offset + length)
       → loop continues on modified text
```

The modified text is fed back into `mdToBlocksBackbone()`, which re-parses the `[label](NOTE/id)` markdown links as `link` AST nodes.

### 2.4 `generateLinkNode()` — The Link Data Structure

`componentHelper.js` lines 188–208:

```ts
function generateLinkNode(toEntity, type, propertyId) {
  return {
    id: toEntity.id,           // entityId being referenced
    link: {
      id: uuid(),              // unique ID for this relationship instance
      type,                    // 'Dependency' (inline link) or 'Database' (property)
      createdAt: ISO_string,
      data: {
        toStructureId: toEntity.structureId,  // type of entity (Note, Person, Tag...)
        propertyId,            // which property, if a property-specific link
      }
    }
  }
}
```

All `@mention` links use `type: 'Dependency'`. The full `LinkToken`:

```ts
{
  type: 'LinkToken',
  id: uuid(),
  text: 'Alice',           // display label (entity title or custom)
  entity: generateLinkNode({ id: 'abc123', structureId: 'Person' }, 'Dependency', undefined),
  url: undefined
}
```

### 2.5 Special Entity Handling

- **`NOTE/` prefix** — if `structureTitle === 'NOTE'`, the entity title is treated as a raw entity ID and loaded directly. This is how already-resolved internal links survive round-trips through the text processor.
- **`UtilDate`** — a structure type where the entity title is a natural-language date string (`"next Monday"`, `"2024-01-15"`). The system parses it and creates a date entity.
- **`allowDailyNoteLinking`** — a flag controlling whether `RootDailyNote` entities can be linked. Off by default in most contexts.

---

## 3. Architectural Differences vs TipTap

### 3.1 Timing: Import vs Real-time

Capacities resolves `@` mentions during **import/paste processing** — it's a batch scan of the raw string. The editor itself likely stores resolved `LinkToken` nodes (with `id` + `structureId` already filled). There is no suggestion popup in this layer.

TipTap must do this in **real-time**: detect trigger → show popup → user picks → insert node. The Suggestion plugin handles detection; the `items()` callback does what Capacities' `findEntity()` does.

### 3.2 Entity Reference Depth

| | Capacities `LinkToken` | TipTap `mention` node (default) |
|-|------------------------|----------------------------------|
| Entity ID | `entity.id` | `attrs.id` |
| Structure/type | `entity.link.data.toStructureId` | not stored |
| Relationship ID | `entity.link.id` (uuid per link) | not stored |
| Display label | `text` | `attrs.label` |
| Trigger char | implicit (all same type) | `attrs.mentionSuggestionChar` |

TipTap's mention node must be extended with `structureId`.

### 3.3 Create vs Reuse

Capacities distinguishes at the text level (prefix char). In TipTap, this maps to the suggestion popup UI — the popup can offer a "Create new…" item at the bottom of the list when no match is found, equivalent to the `/prefix` create path.

---

## 4. TipTap Implementation Requirements

### 4.1 Mention Node — Extended Schema

Fork/extend `@tiptap/extension-mention`:

```ts
addAttributes() {
  return {
    id:          { default: null },   // entityId (Capacities: entity.id)
    label:       { default: null },   // display text (Capacities: text)
    structureId: { default: null },   // entity type (Capacities: toStructureId)
    char:        { default: '@' },    // trigger char ('@' or '#')
  }
}
```

### 4.2 Suggestion Plugin Configuration

Two trigger instances via the `suggestions` array option:

**`@` — link/create entities:**
```ts
{
  char: '@',
  pluginKey: new PluginKey('mention-entity'),
  items: async ({ query }) => {
    const existing = await searchEntities(query)  // → findEntity() equivalent
    const canCreate = query.length > 0
    return [
      ...existing.map(e => ({ id: e.id, label: e.title, structureId: e.structureId })),
      ...(canCreate ? [{ id: null, label: `Create "${query}"`, structureId: null, create: true }] : [])
    ]
  },
  command: ({ editor, range, props }) => {
    if (props.create) {
      // equivalent to context='create': generateEntity() then insert
      createEntityThenInsert(editor, range, props.label)
    } else {
      editor.chain().focus().deleteRange(range)
        .insertContent({ type: 'mention', attrs: { id: props.id, label: props.label, structureId: props.structureId, char: '@' } })
        .run()
    }
  }
}
```

**`#` — tag entities:**
```ts
{
  char: '#',
  pluginKey: new PluginKey('mention-tag'),
  items: async ({ query }) => searchTags(query),  // filterStructureIds: ['RootTag']
  command: ({ editor, range, props }) => {
    editor.chain().focus().deleteRange(range)
      .insertContent({ type: 'mention', attrs: { id: props.id, label: props.label, structureId: 'RootTag', char: '#' } })
      .run()
  }
}
```

**`/` — force-create entity (equivalent to Capacities `/prefix`):**
```ts
{
  char: '/',
  pluginKey: new PluginKey('mention-create'),
  items: async ({ query }) => getCreatableStructures(query),  // structure types user can create
  command: ({ editor, range, props }) => {
    // props.structureId is the chosen structure type
    // always creates a new entity regardless of whether one exists
    createEntityThenInsert(editor, range, props.label, props.structureId)
  }
}
```

### 4.3 Suggestion Popup Lifecycle

Same pattern for all three triggers — shared renderer:

```
user types @ / # / /  →  findSuggestionMatch() active
  →  onStart(props):   mount popup at props.clientRect
  →  onUpdate(props):  re-query items, update popup list
  →  user picks item:  command() → insert node → onExit()
  →  Escape:           onKeyDown returns true → onExit()
```

### 4.4 Async Entity Resolution (Paste / Import)

For paste/import parity with Capacities' `linkEntitiesInText()`, implement a ProseMirror paste transform:

```ts
// Scans pasted text for @Structure/Entity/ patterns (Capacities import regex)
// Resolves each match against the entity backend
// Inserts resolved mention nodes instead of raw text
addProseMirrorPlugins() {
  return [pasteEntityLinkPlugin({ findEntity, generateEntity })]
}
```

This handles the import path. The regex to match is:
```ts
const importMentionRegex = /(?:(@|\/))([^\s\/]+)(?:\/)([^\/]+)(?:\/)/gm
//                          ^-- drop + prefix, keep @ and /
```

---

## 5. Storage & Serialization

### TipTap HTML

```html
<span
  data-type="mention"
  data-id="abc123"
  data-label="Alice"
  data-structure-id="Person"
  data-char="@"
>@Alice</span>
```

### Mapping to Capacities Token

| Capacities `LinkToken` field | TipTap mention attribute |
|-----------------------------|--------------------------|
| `entity.id` | `data-id` |
| `entity.link.data.toStructureId` | `data-structure-id` |
| `text` | `data-label` |
| trigger type (`@` vs `#`) | `data-char` |
| `entity.link.id` (relationship UUID) | not stored — generated on save |
| `entity.link.type` (`'Dependency'`) | implicit — always Dependency for inline mentions |

---

## 6. Implementation Checklist

### Phase 1 — Extended Mention Extension
- [ ] Extend mention node schema with `structureId` and `char` attributes
- [ ] Configure `suggestions` array: `@` (reuse/create), `#` (tag), `/` (force-create)
- [ ] Implement async `items()` for each trigger calling entity backend
- [ ] Implement `command()` for each trigger including create-new path
- [ ] Build shared suggestion popup component (React + tippy.js or floating-ui)

### Phase 2 — Import / Paste Parity
- [ ] ProseMirror paste plugin that detects `@Structure/Entity/` and `/Structure/Entity/` patterns
- [ ] Batch-resolve matches via `findEntity()` / `generateEntity()` before insertion
- [ ] Handle `UtilDate` structure type (natural language date parsing)
- [ ] Handle `NOTE/entityId` direct-ID format for round-trip safety

### Phase 3 — Edge Cases
- [ ] Unresolved mention state (`id=null`) — render with pending/broken visual
- [ ] `allowDailyNoteLinking` flag — gate daily note entities from appearing in `@` results
- [ ] Structure scoping in query — e.g. `@Person/Ali` narrows results to Person structure

---

## 7. Slash Menu

### 7.1 How BlockNote Implements It

BlockNote's slash menu does **not** use `@tiptap/suggestion`. It implements its own ProseMirror plugin via `handleTextInput`, but the architecture is nearly identical to `@tiptap/suggestion` — same decoration-based positioning, same lifecycle, same keyboard pattern. The key difference is **what the command does**: it inserts or replaces a block rather than inline content.

**Trigger detection** (`SuggestionMenu.ts` — `handleTextInput` prop):
1. User types `/`
2. Plugin checks if text matches any registered trigger char
3. Optional `shouldOpen(tr)` callback gates opening (BlockNote blocks `/` inside table cells)
4. If approved: dispatches transaction with plugin state set — active suggestion begins

**Plugin state shape:**
```ts
type SuggestionPluginState = {
  triggerCharacter: string       // '/'
  deleteTriggerCharacter: boolean
  queryStartPos: () => number    // doc position after '/'
  query: string                  // text typed after '/', updates on every keystroke
  decorationId: string           // links plugin state to DOM node for positioning
} | undefined
```

**Query updates:** On every transaction, `query = text between queryStartPos() and cursor`.

**Item schema:**
```ts
type SlashMenuItem = {
  title: string           // "Heading 1"
  onItemClick: () => void // inserts the block
  subtext?: string        // "Top-level heading"
  badge?: string          // "Mod+Alt+1"
  aliases?: string[]      // ["h", "heading1", "h1"] — all searchable
  group?: string          // "Headings" — used for visual grouping
  icon?: ReactElement
}
```

**Filtering:** `title.toLowerCase().includes(query) || aliases.some(a => a.includes(query))`

**Block insertion** (`insertOrUpdateBlockForSlashMenu`):
- If current block is empty or contains only `/`: replace it in-place via `updateBlock()`
- Otherwise: insert a new block below via `insertBlocks()`
- Cursor moved to next editable content

**Positioning:** Decoration placed on the `/` character → `getBoundingClientRect()` → passed to floating-ui (`offset(10)` + `autoPlacement` + `shift` + `size` for max-height clamping).

**Keyboard:**

| Key | Behaviour |
|-----|-----------|
| ArrowDown | next item (wraps) |
| ArrowUp | prev item (wraps) |
| PageDown | last item |
| PageUp | first item |
| Enter | confirm selected item |
| Escape | close menu |

Selection resets to index 0 whenever query changes.

**Close:** Deletes the `/ + query` text from the document via `deleteRange`, sets plugin state to `undefined`.

### 7.2 Popup UI — Visual Implementation

The slash menu and mention popups share the same component structure. BlockNote's three UI packages (Mantine, Ariakit, ShadCN) all follow the same DOM shape — here's the consolidated spec.

#### 7.2.1 DOM Structure

```html
<div class="suggestion-menu" id="bn-suggestion-menu" role="listbox">
  <!-- Group label (repeated per group) -->
  <div class="suggestion-menu-label" role="presentation">
    Headings
  </div>
  
  <!-- Item -->
  <div class="suggestion-menu-item"
       id="suggestion-menu-item-0"
       role="option"
       aria-selected="true|false">
    <!-- Left: icon -->
    <div class="suggestion-menu-item-section" data-position="left">
      <svg>...</svg>
    </div>
    <!-- Center: title + subtext -->
    <div class="suggestion-menu-item-body">
      <div class="suggestion-menu-item-title">Heading 1</div>
      <div class="suggestion-menu-item-subtitle">Top-level heading</div>
    </div>
    <!-- Right: badge (optional) -->
    <div class="suggestion-menu-item-section" data-position="right">
      Mod+Alt+1
    </div>
  </div>
  
  <!-- More items... -->
  
  <!-- Loader (when items are async) -->
  <div class="suggestion-menu-loader">...</div>
  
  <!-- Empty state (when no items match) -->
  <div class="suggestion-menu-empty">No results</div>
</div>
```

#### 7.2.2 Floating-UI Positioning

```ts
useFloating({
  placement: 'bottom-start',   // below the cursor, left-aligned
  middleware: [
    offset(10),                // 10px gap from trigger text
    autoPlacement({
      allowedPlacements: ['bottom-start', 'top-start'],
      padding: 10,             // 10px safety margin before flipping
    }),
    shift(),                   // prevent horizontal overflow
    size({
      apply({ elements, availableHeight }) {
        // Dynamically cap max-height to available viewport space
        elements.floating.style.maxHeight = `${Math.max(0, availableHeight)}px`
      },
      padding: 10,
    }),
  ],
})
```

The decoration placed on the `/` character is the positioning reference — its `getBoundingClientRect()` anchors the popup.

#### 7.2.3 CSS

```css
/* Container */
.suggestion-menu {
  max-height: 100%;           /* constrained by floating-ui size middleware */
  overflow-y: auto;
  box-sizing: border-box;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 4px;
  background: var(--menu-bg, #fff);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

/* Group labels */
.suggestion-menu-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-dimmed);
  padding: 4px 12px;
  cursor: default;
}

/* Items */
.suggestion-menu-item {
  display: flex;
  align-items: center;
  width: 100%;
  min-height: 52px;           /* BlockNote default; use fit-content for compact */
  padding: 8px 12px;
  border-radius: 4px;
  font-size: 14px;
  cursor: default;
  user-select: none;
}

/* Selected / hover state */
.suggestion-menu-item[aria-selected="true"],
.suggestion-menu-item:hover {
  background-color: var(--hover-bg, #efefef);
}

/* Icon section (left) */
.suggestion-menu-item-section[data-position="left"] {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 8px;
  margin-inline-end: 8px;
  background: var(--icon-bg, #efefef);
  border-radius: 4px;
}
.suggestion-menu-item-section[data-position="left"] svg {
  width: 18px;
  height: 18px;
}

/* Body (title + subtitle) */
.suggestion-menu-item-body {
  display: flex;
  flex: 1;
  flex-direction: column;
  padding-right: 16px;
}
.suggestion-menu-item-title {
  font-weight: 500;
  font-size: 14px;
  line-height: 20px;
}
.suggestion-menu-item-subtitle {
  font-size: 10px;
  line-height: 16px;
  color: var(--text-dimmed);
}

/* Badge section (right) */
.suggestion-menu-item-section[data-position="right"] {
  font-size: 11px;
  border-radius: 4px;
  padding-inline: 4px;
  color: var(--text-dimmed);
}

/* Auto-scroll selected item into view */
```

#### 7.2.4 Keyboard Navigation

| Key | Behaviour |
|-----|-----------|
| ArrowDown | `selectedIndex = (idx + 1) % length` (wraps) |
| ArrowUp | `selectedIndex = (idx - 1 + length) % length` (wraps) |
| PageDown | jump to last item |
| PageUp | jump to first item |
| Enter | execute `onItemClick(items[selectedIndex])` |
| Escape | close menu |

Selection resets to index 0 whenever query changes. On selection change, call `scrollIntoView({ block: 'nearest' })` on the item element if it overflows the container.

ARIA: set `aria-expanded="true"` and `aria-activedescendant="suggestion-menu-item-{idx}"` on the editor element while the menu is open.

#### 7.2.5 Auto-scroll

BlockNote checks if the selected item overflows the scroll container and calls `scrollIntoView({ block: 'nearest' })` when selection changes:

```ts
useEffect(() => {
  if (!isSelected || !ref.current) return
  const container = ref.current.closest('.suggestion-menu')
  if (!container) return
  // Check if item is outside visible scroll area
  const itemRect = ref.current.getBoundingClientRect()
  const containerRect = container.getBoundingClientRect()
  if (itemRect.bottom > containerRect.bottom || itemRect.top < containerRect.top) {
    ref.current.scrollIntoView({ block: 'nearest' })
  }
}, [isSelected])
```

### 7.3 Implementation with TipTap Suggestion

For a single TipTap editor, use `@tiptap/suggestion` with `/` as the trigger — the same plugin already used for `@` mentions. No need to replicate BlockNote's custom plugin.

The only difference from a mention is the `command()` callback: instead of inserting an inline node, it dispatches TipTap commands to change or insert a block.

```ts
{
  char: '/',
  pluginKey: new PluginKey('slash-menu'),
  allowSpaces: false,
  shouldShow: ({ editor }) => !editor.isActive('tableCell'),
  items: ({ query }) => filterSlashItems(query, defaultSlashItems),
  command: ({ editor, range, props }) => {
    props.command(editor, range)
  },
  render: () => suggestionPopupRenderer()  // shared with mention — same component
}
```

**Block insertion logic** (from BlockNote's `insertOrUpdateBlockForSlashMenu`):
- If current block is empty or contains only `/`: replace in-place (`deleteRange` + block command)
- Otherwise: insert a new block below
- Cursor moved to next editable content

### 7.4 Slash Menu Item Groups

Render a section label before each group:

```ts
const grouped = items.reduce((acc, item) => {
  const g = item.group ?? 'Other'
  ;(acc[g] ??= []).push(item)
  return acc
}, {})
// Render: group label → items → group label → items
```

---

## 8. Drag Handle

### 8.1 BlockNote SideMenu vs TipTap `extension-drag-handle`

| | BlockNote SideMenu | TipTap `extension-drag-handle` |
|-|--------------------|--------------------------------|
| **Architecture** | PluginView (not a plugin state machine) | ProseMirror plugin, state = `{ locked: boolean }` |
| **Block detection** | `elementsFromPoint` → walk to direct editor child → `getDraggableBlockFromElement()` | `elementsFromPoint` → `findClosestTopLevelBlock()` (non-nested) or scoring-based `findBestDragTarget()` (nested) |
| **Handle positioning** | Read block's `getBoundingClientRect()` directly, emit to UI component | floating-ui `computePosition()` — placement: `left-start`, strategy: `absolute` |
| **Handle DOM** | External — UI component owns the element | External div appended to `editor.view.dom.parentElement`, wrapped by extension |
| **Drag image** | Clones block DOM, inherits CSS classes, filters iframes | Clones DOM with computed styles via `cloneElement()` |
| **ProseMirror integration** | Sets `view.dragging = { slice, move: true }` | Sets `view.dragging = { slice, move: true, node }` |
| **Selection on drag** | Single block → `NodeSelection`; multi-selected → `MultipleNodeSelection` (custom class) | Single node → `NodeSelection`; multi → existing selection range |
| **Drop handling** | Custom `onDrop` handler + synthetic event dispatch for cross-editor | Native ProseMirror drop handler; Firefox `contentEditable` toggle workaround |
| **Drop cursor** | Custom absolute `<div>` overlay, not a ProseMirror decoration | Not included — use `prosemirror-dropcursor` or `@tiptap/extension-dropcursor` separately |
| **Multi-editor support** | Yes — distance calculation (250px gate), synthetic event routing, coordinated deletion | No |
| **mousemove debounce** | Direct handler, `menuFrozen` flag to pause tracking | `requestAnimationFrame` debounce (`rafId`) |
| **Yjs support** | No | Yes — stores relative position, remaps on collab changes |
| **Nested block targeting** | Column-aware: recursive `getBlockFromCoords` with +50px X offset | Scoring system: rules assign scores to ancestor candidates, highest wins |

### 8.2 Handle Alignment — Why TipTap Looks Wrong

The misalignment comes from TipTap's defaults vs BlockNote's layout strategy.

**TipTap `extension-drag-handle` default:**
- Wrapper div at `position: absolute; top: 0; left: 0` inside `editor.view.dom.parentElement`
- floating-ui config: `placement: 'left-start'`, **zero middleware** — no `offset()`
- Reference element: `block.getBoundingClientRect()` — the full block node
- **No editor padding** — the handle has nowhere to sit; it overlaps or floats outside

**BlockNote alignment approach:**
1. **Editor padding reserves space:** `.bn-editor { padding-inline: 54px }` — creates a 54px gutter on each side for the handle to sit in
2. **Reference rect X is pinned to the editor's block-group left edge**, not the individual block. This creates a consistent left column.
3. **Reference rect Y comes from the block content's bounding box** — so the handle's top aligns with the block's first line
4. **CSS height matching per block type:** `.bn-side-menu { height: 30px }` for paragraphs, `height: 78px` for h1, `height: 54px` for h2, etc. — so the handle vertically spans the line height of the first line
5. **FloatingUI `placement: 'left-start'`** — positions handle to the left, aligned with the reference rect's top

**Key insight:** The alignment is achieved by three things working together:
- The editor's left padding creates the gutter
- The reference rect's Y matches the block content top
- The handle height matches the block's line height

### 8.3 Implementation — Aligned Handle

**Editor CSS — create the gutter:**
```css
.ProseMirror {
  padding-inline: 54px;    /* space for handle + small gap */
}
```

**Drag handle setup with offset middleware:**
```ts
import DragHandle from '@tiptap/extension-drag-handle'
import Dropcursor from '@tiptap/extension-dropcursor'
import { offset } from '@floating-ui/dom'

DragHandle.configure({
  render() {
    const el = document.createElement('div')
    el.className = 'drag-handle'
    el.innerHTML = '⠿'  // or a grip SVG icon
    return el
  },

  computePositionConfig: {
    placement: 'left-start',
    strategy: 'absolute',
    middleware: [
      offset({ mainAxis: 0, crossAxis: -8 }),  // fine-tune gap from text
    ],
  },

  // Optional: provide a virtual element that pins X to the editor gutter
  getReferencedVirtualElement({ node, editor, pos }) {
    const blockEl = editor.view.nodeDOM(pos)
    if (!blockEl) return null
    const blockRect = blockEl.getBoundingClientRect()
    const editorRect = editor.view.dom.firstElementChild?.getBoundingClientRect()
    return {
      getBoundingClientRect: () => new DOMRect(
        editorRect?.x ?? blockRect.x,   // pin X to editor left (consistent column)
        blockRect.y,                     // Y from block content
        blockRect.width,
        blockRect.height,
      ),
    }
  },

  onNodeChange({ node, editor, pos }) {
    // optional: update external state for context menu
  },
})
```

**Handle CSS — match line height per block type:**
```css
.drag-handle {
  width: 24px;
  height: 28px;                 /* default: paragraph line height */
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: grab;
  border-radius: 4px;
  opacity: 0;
  transition: opacity 0.15s;
  color: var(--text-dimmed);
}

/* Show on hover — the extension manages visibility via onNodeChange */
.drag-handle:hover,
.drag-handle[data-dragging="true"] {
  opacity: 1;
  background: var(--hover-bg, #efefef);
}

/* Height adjustments for different block types */
/* Apply via onNodeChange callback setting a data attribute */
.drag-handle[data-block-type="heading"][data-level="1"] { height: 48px; }
.drag-handle[data-block-type="heading"][data-level="2"] { height: 40px; }
.drag-handle[data-block-type="heading"][data-level="3"] { height: 34px; }
```

**Setting block type on the handle** (via `onNodeChange`):
```ts
onNodeChange({ node, editor, pos }) {
  const handleEl = document.querySelector('.drag-handle')
  if (!handleEl) return
  handleEl.dataset.blockType = node.type.name
  if (node.attrs.level) handleEl.dataset.level = node.attrs.level
}
```

### 8.4 Visibility Behaviour

BlockNote shows the handle when the mouse is near the editor and hides it on keypress. Replicate with TipTap:

```css
/* Show handle when editor wrapper is hovered */
.editor-wrapper:hover .drag-handle { opacity: 0.5; }
.editor-wrapper:hover .drag-handle:hover { opacity: 1; }
```

The `extension-drag-handle` already hides the handle via `hideDragHandle` meta on certain events. The `locked` option freezes position when a context menu is open.

### 8.5 Drop Cursor

Add `@tiptap/extension-dropcursor` for the visual drop indicator:

```ts
Dropcursor.configure({
  color: '#aabbdd',
  width: 2,
})
```

BlockNote's drop cursor is a standalone `<div>` overlay (not a ProseMirror decoration). If the standard dropcursor is insufficient (e.g. need different visuals between block-level and inline drops), replicate BlockNote's approach:
- `onDragOver`: `view.posAtCoords()` → resolve position → `!$pos.parent.inlineContent` = block-level
- Position an absolute div between blocks for block drops, or as a thin vertical line for inline drops
- Remove on `drop`/`dragend` after 20ms

---

## 9. Updated Implementation Checklist

### Phase 1 — Mention Extension
- [ ] Extend mention node schema: `id`, `label`, `structureId`, `char`
- [ ] Three suggestion configs: `@` (reuse/create entity), `#` (tag), `/force-create`
- [ ] Async `items()` calling entity backend; `#` filters to `RootTag` structure
- [ ] `command()` with create-new path for `@` and `/`
- [ ] Shared floating popup component (floating-ui + keyboard handler)

### Phase 2 — Slash Menu
- [ ] Add `/` suggestion config to shared suggestion setup
- [ ] Define slash item list: headings, lists, paragraph, code block, divider, etc.
- [ ] `filterSlashItems()` utility (title + aliases)
- [ ] Group headers in popup rendering
- [ ] `command()` dispatches TipTap chain: `deleteRange` + block command
- [ ] `shouldShow` blocks slash menu inside table cells

### Phase 3 — Drag Handle (Aligned)
- [ ] Add `padding-inline: 54px` to `.ProseMirror` to create the handle gutter
- [ ] Add `@tiptap/extension-drag-handle` with custom `render()` for grip icon
- [ ] Configure `computePositionConfig` with `offset` middleware for fine-tuning
- [ ] Implement `getReferencedVirtualElement` — pin X to editor left edge, Y to block top
- [ ] Handle CSS: height matches per block type via `data-block-type` attribute
- [ ] `onNodeChange` sets `data-block-type` and `data-level` on handle element
- [ ] Add `@tiptap/extension-dropcursor` for visual drop indicator
- [ ] `locked` flag toggled when context menu is open (prevent handle jumping)
- [ ] Show/hide handle via CSS hover + editor wrapper

### Phase 4 — Import Parity
- [ ] Paste transform plugin: detect `@Structure/Entity/` and `/Structure/Entity/`
- [ ] Batch-resolve via `findEntity()` / `generateEntity()`
- [ ] `NOTE/entityId` round-trip format
- [ ] Unresolved mention state (`id=null`) — pending visual
