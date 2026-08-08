# Spec: Drag handle

Block reordering handle in the editor's left gutter, **as built**. The comparison
against BlockNote's SideMenu that informed it is in
[`../research/blocknote-vs-tiptap.md`](../research/blocknote-vs-tiptap.md).

**Code:** `calcifer/src/editors/tiptap/TiptapEditor.tsx`, `TiptapEditor.css`.

---

## Wiring

The React wrapper from `@tiptap/extension-drag-handle-react`, mounted inside the scroll
container and *before* `EditorContent`:

```tsx
<div className="min-h-0 flex-1 overflow-y-auto">
  <DragHandleReact editor={editor}>
    <DotsSixVerticalIcon weight="bold" />
  </DragHandleReact>
  <EditorContent editor={editor} className="h-full" />
</div>
```

No `computePositionConfig`, no `getReferencedVirtualElement`, no `onNodeChange` — the
extension's defaults are used as-is.

## Alignment

Two things do the work.

**1. The gutter** — editor padding reserves space for the handle, set through
`editorProps` rather than CSS:

```ts
editorProps: {
  attributes: { class: 'outline-none min-h-full px-16 py-10 text-base leading-normal' },
}
```

`px-16` is `4rem` / 64px each side. This is the equivalent of BlockNote's
`padding-inline: 54px`, and it is the load-bearing part: without it the handle has
nowhere to sit and overlaps the text.

**2. A CSS transform** for the final nudge:

```css
.drag-handle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 1.25rem;
  height: 1.25rem;
  border-radius: var(--radius-sm);
  color: var(--muted-foreground);
  cursor: grab;
  transition: color 0.1s, background-color 0.1s;
  transform: translate(-2px, 2px);
}

.drag-handle:hover  { color: var(--foreground); background-color: var(--accent); }
.drag-handle:active { cursor: grabbing; }
```

> **Do not reach for floating-ui `offset` middleware here.** The research notes
> prescribed `computePositionConfig.middleware: [offset({ mainAxis: 0, crossAxis: -8 })]`
> plus a `getReferencedVirtualElement` pinning X to the editor edge. That route was
> tried and abandoned; alignment is done with the CSS transform on `.drag-handle`
> instead. Re-introducing the middleware re-opens a solved problem.

## What isn't done

The researched BlockNote approach also matched handle height per block type, so the
handle spans the first line of an `h1` differently than a paragraph:

```css
.drag-handle[data-block-type="heading"][data-level="1"] { height: 48px; }
```

driven by an `onNodeChange` callback stamping `data-block-type` / `data-level` onto the
handle element. **Not implemented.** The handle is a fixed `1.25rem` square, vertically
centred by its own flex rules. Alignment against large headings is therefore
approximate — the visible symptom if it ever needs revisiting.

## Visibility

Handled entirely by the extension's own show/hide behaviour plus the `:hover` rules
above. There is no `.editor-wrapper:hover` opacity rule, and the `locked` option is not
used.

## Drop cursor

Provided by StarterKit's bundled `Dropcursor` — not configured separately, so it uses
default colour and width. `@tiptap/extension-dropcursor` is a direct dependency but is
not separately registered in the extension list.
