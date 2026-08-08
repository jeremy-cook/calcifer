# Research: BlockNote vs TipTap — slash menu and drag handle

> **Frozen — historical.** Notes from ~2026-04, read out of
> `~/Github/BlockNote/packages/core/src/extensions/` and `~/Github/tiptap/packages/`
> while deciding how to build Calcifer's editor. This describes **BlockNote's and
> TipTap's code, not ours.** Not maintained. For what Calcifer actually does see
> [`../specs/slash-menu.md`](../specs/slash-menu.md) and
> [`../specs/drag-handle.md`](../specs/drag-handle.md).

---

## Why not use BlockNote

BlockNote is roughly 60–70% custom logic on top of TipTap — its own schema, block
model, block-level operations, and a full UI layer. That's a lot of surface to adopt for
a single editor, and it constrains the schema, which for Calcifer needs to carry entity
mentions and date chips.

TipTap's first-party extensions (`extension-drag-handle`, `extension-dropcursor`) plus
the `suggestion` package cover the same ground directly. **Decision: build on TipTap,
mine BlockNote for implementation detail.**

---

## Slash menu

BlockNote does **not** use `@tiptap/suggestion`. It implements its own ProseMirror
plugin via `handleTextInput` — but architecturally it is nearly identical: same
decoration-based positioning, same lifecycle, same keyboard pattern. The meaningful
difference is only what the command does (insert/replace a *block* rather than inline
content).

**Conclusion: no need to replicate it.** Configure `@tiptap/suggestion` with `/`.

Plugin state shape, for reference:

```ts
type SuggestionPluginState = {
  triggerCharacter: string       // '/'
  deleteTriggerCharacter: boolean
  queryStartPos: () => number    // doc position after '/'
  query: string                  // text after '/', updates per keystroke
  decorationId: string           // links plugin state to DOM node for positioning
} | undefined
```

Item shape:

```ts
type SlashMenuItem = {
  title: string
  onItemClick: () => void
  subtext?: string
  badge?: string          // "Mod+Alt+1"
  aliases?: string[]      // all searchable
  group?: string
  icon?: ReactElement
}
```

Filtering: `title.toLowerCase().includes(query) || aliases.some(a => a.includes(query))`.

**Block insertion** (`insertOrUpdateBlockForSlashMenu`) — worth noting because Calcifer
does *not* do this:

- If the current block is empty or contains only `/`: replace it in place via
  `updateBlock()`.
- Otherwise: insert a new block below via `insertBlocks()`.
- Move the cursor to the next editable content.

**Trigger gating:** an optional `shouldOpen(tr)` callback; BlockNote uses it to block
`/` inside table cells.

**Positioning:** decoration on the `/` character → `getBoundingClientRect()` →
floating-ui with `offset(10)` + `autoPlacement` + `shift` + `size` (the last clamping
`max-height` to available viewport space).

**Keyboard:** ArrowDown/Up wrap; PageDown/PageUp jump to last/first; Enter confirms;
Escape closes. Selection resets to index 0 whenever the query changes, and the selected
item is scrolled into view with `scrollIntoView({ block: 'nearest' })` when it overflows
the container.

### Popup DOM shape

All three BlockNote UI packages (Mantine, Ariakit, ShadCN) converge on this structure:

```html
<div class="suggestion-menu" role="listbox">
  <div class="suggestion-menu-label" role="presentation">Headings</div>
  <div class="suggestion-menu-item" role="option" aria-selected="true|false">
    <div class="suggestion-menu-item-section" data-position="left"><svg/></div>
    <div class="suggestion-menu-item-body">
      <div class="suggestion-menu-item-title">Heading 1</div>
      <div class="suggestion-menu-item-subtitle">Top-level heading</div>
    </div>
    <div class="suggestion-menu-item-section" data-position="right">Mod+Alt+1</div>
  </div>
  <div class="suggestion-menu-loader">…</div>
  <div class="suggestion-menu-empty">No results</div>
</div>
```

Notable dimensions: item `min-height: 52px`, padding `8px 12px`, title 14px/500,
subtitle 10px dimmed, icon 18px on a tinted 4px-radius square, container `padding: 4px`
with a 6px radius and `overflow-y: auto`.

ARIA: `aria-expanded="true"` and
`aria-activedescendant="suggestion-menu-item-{idx}"` on the editor while open.

---

## Drag handle

| | BlockNote SideMenu | TipTap `extension-drag-handle` |
|-|--------------------|--------------------------------|
| **Architecture** | PluginView (not a state machine) | ProseMirror plugin, state = `{ locked: boolean }` |
| **Block detection** | `elementsFromPoint` → walk to direct editor child → `getDraggableBlockFromElement()` | `elementsFromPoint` → `findClosestTopLevelBlock()`, or scoring-based `findBestDragTarget()` when nested |
| **Handle positioning** | Reads the block's `getBoundingClientRect()` directly, emits to the UI component | floating-ui `computePosition()`, `placement: 'left-start'`, `strategy: 'absolute'` |
| **Handle DOM** | External — the UI component owns it | External div appended to `editor.view.dom.parentElement` |
| **Drag image** | Clones block DOM, inherits CSS classes, filters iframes | Clones DOM with computed styles via `cloneElement()` |
| **PM integration** | `view.dragging = { slice, move: true }` | `view.dragging = { slice, move: true, node }` |
| **Selection on drag** | `NodeSelection`, or a custom `MultipleNodeSelection` | `NodeSelection`, or the existing selection range |
| **Drop handling** | Custom `onDrop` + synthetic events for cross-editor | Native PM drop handler; Firefox `contentEditable` workaround |
| **Drop cursor** | Custom absolute `<div>` overlay | Not included — use `prosemirror-dropcursor` separately |
| **Multi-editor** | Yes — 250px distance gate, event routing, coordinated deletion | No |
| **mousemove** | Direct handler + `menuFrozen` flag | `requestAnimationFrame` debounce |
| **Yjs** | No | Yes — relative position, remapped on collab changes |
| **Nested targeting** | Column-aware recursive `getBlockFromCoords` (+50px X offset) | Scoring system over ancestor candidates |

### Why the TipTap handle looks misaligned out of the box

**TipTap defaults:**

- Wrapper at `position: absolute; top: 0; left: 0` in `editor.view.dom.parentElement`
- floating-ui `placement: 'left-start'` with **zero middleware**
- Reference element is `block.getBoundingClientRect()` — the whole block
- **No editor padding**, so the handle has nowhere to sit and overlaps or floats outside

**BlockNote's approach**, three parts working together:

1. `.bn-editor { padding-inline: 54px }` reserves a gutter.
2. Reference rect **X pinned to the editor block-group left edge**, not the individual
   block — giving a consistent left column.
3. Reference rect **Y from the block content's box**, so the handle tops out with the
   block's first line; handle **height CSS-matched per block type**
   (`30px` paragraph, `78px` h1, `54px` h2) so it spans that first line.

> **How Calcifer resolved this:** the gutter (part 1) was adopted — `px-16` on the
> ProseMirror element. Parts 2 and 3 were **not**. Fine alignment is a
> `transform: translate(-2px, 2px)` on `.drag-handle`; the floating-ui `offset`
> middleware route was tried and abandoned. See
> [`../specs/drag-handle.md`](../specs/drag-handle.md).

### Drop cursor

If `@tiptap/extension-dropcursor` proves insufficient (e.g. different visuals for
block-level vs inline drops), BlockNote's approach was:

- `onDragOver`: `view.posAtCoords()` → resolve → `!$pos.parent.inlineContent` means
  block-level
- Absolutely-positioned div between blocks for block drops, thin vertical line for
  inline
- Removed on `drop`/`dragend` after 20ms
