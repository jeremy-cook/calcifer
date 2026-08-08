# Spec: Slash menu (`/`)

How the block-insertion menu works **as built**, including the suggestion popup shared
with mentions. Prior-art research is in
[`../research/blocknote-vs-tiptap.md`](../research/blocknote-vs-tiptap.md).

**Code:** `calcifer/src/lib/tiptap-extension-slash-command/`,
`editors/tiptap/components/slash-menu/*`, `components/suggestionPopup.ts`.

---

## Scope

`/` inserts **blocks only**. It never creates entities — that's `@` and `#`
([`mentions.md`](mentions.md)). Keeping triggers single-purpose is deliberate: an
earlier design had `/Structure/Entity/` as a create syntax, and it was dropped.

---

## Extension

`SlashCommand` is a local extension wrapping `@tiptap/suggestion`:

```ts
SlashCommand.configure({ suggestion: slashSuggestion })
```

Its `command` just delegates to the item:

```ts
command: ({ editor, range, props }) => props.command({ editor, range })
```

so each item owns its own insertion behaviour and is responsible for its own
`deleteRange(range)`.

## Items

`SLASH_ITEMS` in `items.ts`. Shape:

```ts
interface SlashCommandItem {
  title: string
  group: string
  searchTerms?: string[]
  icon: ...
  command: (props: { editor: Editor; range: Range }) => void
}
```

Groups, in render order: **Inline**, **Text**, **Heading**, **List**, **Table**,
**Media**.

The Inline group holds the date shortcuts, which are the one case where `/` inserts
something inline rather than a block:

```ts
command: ({ editor, range }) =>
  editor.chain().focus().deleteRange(range).insertDateChip(todayIso()).run()
```

`/today`, `/tomorrow`, `/yesterday` insert a chip for the corresponding ISO day;
`/Date` opens the picker for an arbitrary day.

## Filtering

```ts
items: ({ query }) => {
  if (!query) return SLASH_ITEMS
  const q = query.toLowerCase()
  return SLASH_ITEMS.filter(item =>
    item.title.toLowerCase().includes(q) ||
    item.group.toLowerCase().includes(q) ||
    item.searchTerms?.some(t => t.includes(q)),
  ).slice(0, 12)
}
```

Group name is searchable, so `/head` surfaces every heading. Capped at 12 results;
empty query shows the full list.

> `searchTerms` entries are compared without lower-casing the term itself — they must
> be authored lowercase to match.

---

## Popup

`createSuggestionPopup(MenuComponent)` in `components/suggestionPopup.ts` is shared by
the slash menu (`SlashCommandMenu`) and both mention triggers (`MentionMenu`). It
returns a `render` implementing the TipTap suggestion lifecycle.

**Positioning** — floating-ui `computePosition` against a virtual element derived from
the current selection:

```ts
const virtualElement = {
  getBoundingClientRect: () =>
    posToDOMRect(editor.view, editor.state.selection.from, editor.state.selection.to),
}
computePosition(virtualElement, element, {
  placement: 'bottom-start',
  strategy: 'absolute',
  middleware: [shift(), flip()],
})
```

The element is appended to `document.body`, so it escapes editor overflow clipping.

**Lifecycle:**

| Hook | Behaviour |
|---|---|
| `onStart` | Mount `ReactRenderer`, append to body, position |
| `onUpdate` | Push new props, reposition |
| `onKeyDown` | `Escape` destroys and removes the popup, returns `true`; anything else delegates to the menu component's `onKeyDown` |
| `onExit` | Destroy and remove |

Arrow-key navigation and Enter selection live in the menu components, which expose
`onKeyDown` through a `SuggestionMenuHandle` ref.

### Divergence from the researched design

The BlockNote popup used `offset(10)` for a gap, `autoPlacement` restricted to
`bottom-start`/`top-start`, and a `size()` middleware clamping `max-height` to the
available viewport space. Ours uses only `shift()` and `flip()`. Consequences:

- No gap between the caret and the popup.
- No dynamic max-height, so a long list near the viewport bottom can overflow rather
  than scroll within a clamped container.

Worth revisiting if the menu grows.

---

## Known gaps

- **No table-cell suppression.** The research specced `shouldShow: ({ editor }) =>
  !editor.isActive('tableCell')` to block `/` inside table cells, matching BlockNote.
  It is not implemented — the menu opens inside cells.
- **No in-place block replacement.** BlockNote replaces the current block when it is
  empty or contains only `/`, and otherwise inserts below. Our items each call
  `deleteRange` and insert, without that empty-block distinction.
- **No group headers with counts or badges**; items render title + icon only.
