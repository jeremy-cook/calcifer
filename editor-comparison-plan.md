# Editor Comparison Plan: Tiptap vs Lexical

## Goal

Build a side-by-side comparative implementation of **Tiptap** and **Lexical** in the Calcifer app. Both editors are implemented incrementally through the same phases, targeting feature parity with the [Lexical Playground](https://github.com/facebook/lexical/tree/main/packages/lexical-playground) as the eventual ceiling.

The result is a live, interactive case study: toggle between the two editors, try the same features, and observe the DX differences as complexity grows.

---

## UI Shell

A tab switcher on the home page (`/`) with two tabs: **Tiptap** and **Lexical**. The active editor fills the full viewport width below the tab bar. Both editors are always mounted (no remount on tab switch) so state is preserved while toggling.

```
┌────────────────────────────────────────────────┐
│  [ Tiptap ]  [ Lexical ]                       │
├────────────────────────────────────────────────┤
│                                                │
│              Active Editor                     │
│           (full viewport width)                │
│                                                │
└────────────────────────────────────────────────┘
```

---

## Installation

### Tiptap
```bash
pnpm add @tiptap/react @tiptap/pm @tiptap/starter-kit
```

### Lexical
```bash
pnpm add lexical @lexical/react
```

Additional packages are listed per phase below.

---

## Phases

Each phase is implemented in **both** editors before moving to the next. This keeps the comparison honest and surfaces DX differences at each layer of complexity.

---

### Phase 1 — Base Editor Shell

**Goal:** Both editors render with no content, accept keyboard input, and persist basic plain text.

**Tiptap packages:** `@tiptap/react`, `@tiptap/starter-kit` (includes Document, Paragraph, Text, Bold, Italic, History)

**Lexical packages:** `lexical`, `@lexical/react` (LexicalComposer, ContentEditable, PlainTextPlugin or RichTextPlugin, HistoryPlugin)

**Features:**
- [X] Empty editor with placeholder text
- [X] Plain text input
- [X] Undo / Redo (Cmd+Z / Cmd+Shift+Z)

**File layout:**
```
src/
  editors/
    tiptap/
      TiptapEditor.tsx         ← top-level component
    lexical/
      LexicalEditor.tsx        ← top-level component
  pages/
    Home.tsx                   ← tab switcher + mounts both editors
```

---

### Phase 2 — Core Text Formatting

**Goal:** Basic inline formatting with keyboard shortcuts and a minimal toolbar.

**Tiptap packages:** already in `starter-kit` (Bold, Italic, Strike, Code); add `@tiptap/extension-underline`

**Lexical packages:** `@lexical/rich-text`, `@lexical/selection`, `@lexical/utils`

**Features:**
- [X] Bold (`Cmd+B`)
- [X] Italic (`Cmd+I`)
- [X] Underline (`Cmd+U`)
- [X] Strikethrough
- [X] Inline code
- [X] Toolbar: buttons for each mark, active state highlights the current selection

---

### Phase 3 — Extended Formatting

**Goal:** Full typographic controls matching the Lexical playground toolbar.

**Tiptap packages:** `@tiptap/extension-text-style`, `@tiptap/extension-color`, `@tiptap/extension-highlight`, `@tiptap/extension-subscript`, `@tiptap/extension-superscript`, `@tiptap/extension-font-family`, `tiptap-extension-font-size` (community)

**Lexical packages:** built-in via `$getSelection`, `$patchStyleText`

**Features:**
- [X] Text highlight / background color
- [X] Font color picker
- [X] Font family selector (Arial, Georgia, Courier New, Trebuchet MS, Verdana)
- [X] Subscript / Superscript
- [X] Clear formatting command

---

### Phase 4 — Block Types & Alignment

**Goal:** All heading levels, quote, alignment, indent/outdent.

**Tiptap packages:** already in `starter-kit` (Heading, Blockquote); add `@tiptap/extension-text-align`

**Lexical packages:** `@lexical/rich-text` (HeadingNode, QuoteNode), built-in FORMAT_ELEMENT_COMMAND

**Features:**
- [X] Paragraph / Normal
- [X] Heading 1, 2, 3
- [X] Block quote
- [X] Left / Center / Right / Justify / Start / End alignment
- [X] Block type dropdown in toolbar

---

### Phase 5 — Lists

**Tiptap packages:** `@tiptap/extension-list-item`, `@tiptap/extension-ordered-list`, `@tiptap/extension-bullet-list`, `@tiptap/extension-task-list`, `@tiptap/extension-task-item`

**Lexical packages:** `@lexical/list` (ListPlugin, ListNode, ListItemNode, CheckListPlugin)

**Features:**
- [X] Bullet list
- [X] Numbered list
- [X] Checklist / Task list
- [X] Nested list support
- [X] Keyboard shortcuts (Tab to nest, Shift+Tab to unnest)

---

### Phase 6 — Code Block with Syntax Highlighting

**Tiptap packages:** `@tiptap/extension-code-block-lowlight`, `lowlight`, language packs

**Lexical packages:** `@lexical/code` (CodeNode, CodeHighlightNode, CodeHighlightPlugin)

**Features:**
- [X] Fenced code block
- [X] Language selector dropdown
- [X] Syntax highlighting (Prism / lowlight)
- [X] Tab key inserts spaces (not navigation) inside code blocks
- [X] Code action menu (copy button)

---

### Phase 7 — Links

**Tiptap packages:** `@tiptap/extension-link`

**Lexical packages:** `@lexical/link` (LinkNode, AutoLinkNode, LinkPlugin, AutoLinkPlugin)

**Features:**
- [ ] Insert / edit / remove hyperlinks
- [ ] Floating link editor (click link → popover with edit/remove)
- [ ] Auto-link detection (URLs become links as you type)
- [ ] Link keyboard shortcut (`Cmd+K`)

---

### Phase 8 — Tables

**Tiptap packages:** `@tiptap/extension-table`, `@tiptap/extension-table-row`, `@tiptap/extension-table-header`, `@tiptap/extension-table-cell`

**Lexical packages:** `@lexical/table` (TablePlugin, TableNode, TableCellNode, TableRowNode)

**Features:**
- [ ] Insert table (via toolbar or slash menu)
- [ ] Add / remove rows and columns
- [ ] Cell merge / split
- [ ] Cell background colors
- [ ] Table action menu (right-click or hover)
- [ ] Nested tables

---

### Phase 9 — Images & Media

**Tiptap packages:** `@tiptap/extension-image`; for resize: `tiptap-extension-resize-image` (community)

**Lexical packages:** custom `ImageNode`, `ImagePlugin`

**Features:**
- [ ] Insert image (URL or file upload)
- [ ] Resize handles
- [ ] Image caption (inline editor below image)
- [ ] Paste image from clipboard
- [ ] GIF insertion

---

### Phase 10 — Embeds (YouTube + Excalidraw)

**Tiptap packages:** `@tiptap/extension-youtube`; Excalidraw requires custom node view

**Lexical packages:** `YouTubePlugin` with `YouTubeNode`, `ExcalidrawPlugin` with custom `ExcalidrawNode` (decorator node)

**Features:**
- [ ] YouTube embed (paste URL → auto-embed)
- [ ] Insert Excalidraw interactive diagram block
- [ ] Open Excalidraw modal to draw / edit
- [ ] Diagram persisted as JSON in the node

---

### Phase 11 — Floating Toolbar & Bubble Menu

**Tiptap packages:** `@tiptap/extension-bubble-menu`, `@tiptap/extension-floating-menu`

**Lexical packages:** custom `FloatingTextFormatPlugin` using `createPortal` + selection events

**Features:**
- [ ] Floating toolbar appears on text selection (desktop)
- [ ] Toolbar includes: Bold, Italic, Underline, Code, Link, Format type, Color
- [ ] Floating menu for empty lines (insert block commands)

---

### Phase 12 — Mentions & Hashtags

**Tiptap packages:** `@tiptap/extension-mention`, `@tiptap/suggestion`

**Lexical packages:** `@lexical/react` `MentionsPlugin` with custom `MentionNode`

**Features:**
- [ ] `@mention` — triggers suggestion popup, inserts mention node
- [ ] `#hashtag` — triggers suggestion popup, inserts hashtag node
- [ ] Suggestion popup: fuzzy search, keyboard navigation
- [ ] Mention node rendered as styled chip

---

### Phase 13 — Emoji Picker

**Tiptap packages:** community `tiptap-extension-emoji` + `emoji-picker-element` or similar

**Lexical packages:** `@lexical/react` `EmojiPlugin` with `EmojiNode`

**Features:**
- [ ] `:` trigger → emoji search popup
- [ ] Toolbar button → full emoji picker modal
- [ ] Emoji rendered as custom node

---

### Phase 14 — Slash Command Menu

**Tiptap packages:** `@tiptap/suggestion` (configure `/` as trigger)

**Lexical packages:** custom plugin using `COMMAND_PRIORITY_LOW` + `TextNode` transform

**Features:**
- [ ] `/` triggers command picker
- [ ] Groups: Text, Heading, List, Table, Media, Embeds
- [ ] Fuzzy search / filter as you type
- [ ] Keyboard navigation (↑↓ select, Enter confirm, Escape dismiss)
- [ ] Auto-scroll within dropdown
- [ ] Item: icon + title + subtitle

---

### Phase 15 — Drag & Drop Block Reordering

**Tiptap packages:** `@tiptap/extension-drag-handle` (Pro) or community alternative

**Lexical packages:** `@lexical/react` `DraggableBlockPlugin`

**Features:**
- [ ] Drag handle appears on block hover (left of block, vertically centered)
- [ ] Drag to reorder blocks
- [ ] Visual drop indicator between blocks
- [ ] Editor horizontal padding aligns content with handle

---

### Phase 16 — Math / Equations

**Tiptap packages:** `@tiptap/extension-mathematics` (Pro) or community `tiptap-math`

**Lexical packages:** `@lexical/react` `EquationPlugin` with KaTeX

**Features:**
- [ ] Insert inline or block equation
- [ ] KaTeX rendering
- [ ] Click to edit raw LaTeX source

---

### Phase 17 — Table of Contents

**Tiptap packages:** `@tiptap/extension-table-of-contents`

**Lexical packages:** custom plugin scanning `HeadingNode`s

**Features:**
- [ ] Auto-generated TOC sidebar from heading nodes
- [ ] Clicking jumps to heading
- [ ] Updates live as headings change

---

### Phase 18 — Advanced Containers

**Tiptap packages:** `@tiptap/extension-details` (community), custom column layout node view

**Lexical packages:** `CollapsiblePlugin` (CollapsibleContainerNode, CollapsibleTitleNode, CollapsibleContentNode), custom `LayoutPlugin`

**Features:**
- [ ] Collapsible / expandable containers
- [ ] Multi-column layout (2-col, 3-col)
- [ ] Page break node
- [ ] Sticky notes (floating draggable note blocks)

---

### Phase 19 — Markdown Import / Export

**Tiptap packages:** `@tiptap/extension-markdown` (community)

**Lexical packages:** `@lexical/markdown` (TRANSFORMERS, `$convertToMarkdownString`, `$convertFromMarkdownString`, `MarkdownShortcutPlugin`)

**Features:**
- [ ] Markdown shortcuts (e.g., `##` → Heading 2, `- ` → bullet list)
- [ ] Export editor content as Markdown string
- [ ] Import Markdown string into editor

---

### Phase 20 — HTML Import / Export

**Tiptap packages:** `editor.getHTML()` / `editor.commands.setContent(html)` built-in

**Lexical packages:** `@lexical/html` (`$generateHtmlFromNodes`, `$generateNodesFromDOM`)

**Features:**
- [ ] Export to HTML string
- [ ] Import / paste HTML with style preservation

---

### Phase 21 — Character Count & Constraints

**Tiptap packages:** `@tiptap/extension-character-count`

**Lexical packages:** custom plugin using `editor.registerUpdateListener`

**Features:**
- [ ] Live character and word count display
- [ ] Hard character limit with enforcement
- [ ] UTF-8 vs UTF-16 count modes

---

### Phase 22 — Versioning & History

**Goal:** Beyond undo/redo — named snapshots.

**Tiptap:** manual snapshot approach (serialize to JSON + store array)

**Lexical:** `VersionPlugin` (custom in playground)

**Features:**
- [ ] Save named version / snapshot
- [ ] Browse version history
- [ ] Restore to a prior version

---

## Feature Comparison Matrix

This matrix maps every Lexical playground feature to its Tiptap equivalent and tracks implementation status. ✅ = done, 🔶 = partial/workaround, ❌ = not available, ⬜ = not yet started.

| Feature | Phase | Tiptap | Lexical | Notes |
|---------|-------|--------|---------|-------|
| Plain text input | 1 | ⬜ | ⬜ | |
| Undo / Redo | 1 | ⬜ | ⬜ | |
| Bold | 2 | ⬜ | ⬜ | |
| Italic | 2 | ⬜ | ⬜ | |
| Underline | 2 | ⬜ | ⬜ | |
| Strikethrough | 2 | ⬜ | ⬜ | |
| Inline code | 2 | ⬜ | ⬜ | |
| Highlight / Background color | 3 | ⬜ | ⬜ | |
| Font color | 3 | ⬜ | ⬜ | |
| Font family | 3 | ⬜ | ⬜ | |
| Font size | 3 | ⬜ | ⬜ | |
| Subscript | 3 | ⬜ | ⬜ | |
| Superscript | 3 | ⬜ | ⬜ | |
| Clear formatting | 3 | ⬜ | ⬜ | |
| Paragraph | 4 | ⬜ | ⬜ | |
| Heading 1/2/3 | 4 | ⬜ | ⬜ | |
| Block quote | 4 | ⬜ | ⬜ | |
| Text alignment (6 modes) | 4 | ⬜ | ⬜ | |
| Bullet list | 5 | ⬜ | ⬜ | |
| Numbered list | 5 | ⬜ | ⬜ | |
| Checklist / Task list | 5 | ⬜ | ⬜ | |
| Nested lists | 5 | ⬜ | ⬜ | |
| Code block | 6 | ⬜ | ⬜ | |
| Language selector | 6 | ⬜ | ⬜ | |
| Syntax highlighting | 6 | ⬜ | ⬜ | |
| Tab in code block | 6 | ⬜ | ⬜ | |
| Code copy button | 6 | ⬜ | ⬜ | |
| Hyperlinks | 7 | ⬜ | ⬜ | |
| Floating link editor | 7 | ⬜ | ⬜ | |
| Auto-link | 7 | ⬜ | ⬜ | |
| Table insert | 8 | ⬜ | ⬜ | |
| Table row/col add-remove | 8 | ⬜ | ⬜ | |
| Cell merge / split | 8 | ⬜ | ⬜ | |
| Cell background color | 8 | ⬜ | ⬜ | |
| Nested tables | 8 | ⬜ | ⬜ | Lexical supports; Tiptap limited |
| Image insert | 9 | ⬜ | ⬜ | |
| Image resize | 9 | ⬜ | ⬜ | Tiptap needs community extension |
| Image caption | 9 | ⬜ | ⬜ | Lexical has native nested editor |
| Paste image | 9 | ⬜ | ⬜ | |
| YouTube embed | 10 | ⬜ | ⬜ | Tiptap has first-party extension |
| Excalidraw embed | 10 | ⬜ | ⬜ | Both need custom node view |
| Floating toolbar | 11 | ⬜ | ⬜ | |
| `@mention` | 12 | ⬜ | ⬜ | |
| `#hashtag` | 12 | ⬜ | ⬜ | |
| Emoji picker | 13 | ⬜ | ⬜ | |
| Slash command menu | 14 | ⬜ | ⬜ | |
| Drag & drop blocks | 15 | ⬜ | ⬜ | Tiptap Pro only for first-party |
| Math / KaTeX equations | 16 | ⬜ | ⬜ | Tiptap Pro for first-party |
| Table of contents | 17 | ⬜ | ⬜ | |
| Collapsible containers | 18 | ⬜ | ⬜ | |
| Multi-column layout | 18 | ⬜ | ⬜ | |
| Sticky notes | 18 | ⬜ | ⬜ | Lexical-specific concept |
| Markdown shortcuts | 19 | ⬜ | ⬜ | |
| Markdown export | 19 | ⬜ | ⬜ | |
| Markdown import | 19 | ⬜ | ⬜ | |
| HTML export | 20 | ⬜ | ⬜ | |
| HTML import | 20 | ⬜ | ⬜ | |
| Character count | 21 | ⬜ | ⬜ | |
| Character limit | 21 | ⬜ | ⬜ | |
| Version history | 22 | ⬜ | ⬜ | |
| Polls | — | 🔶 | ⬜ | Tiptap: custom node view only |
| Speech-to-text | — | ❌ | ⬜ | Lexical playground only |
| Debug tree view | — | ❌ | ⬜ | Lexical devtools only |
| RTL text direction | — | ⬜ | ⬜ | |
| Split screen view | — | ❌ | ⬜ | Lexical playground only |

---

## Key DX Observations to Watch

These are hypotheses to validate as we build. Update with findings as each phase completes.

| Dimension | Hypothesis |
|-----------|------------|
| **Onboarding** | Tiptap's `StarterKit` one-liner wins Phase 1; Lexical requires more wiring |
| **Custom nodes** | Lexical's node/decorator pattern is more explicit; Tiptap's NodeView is more React-idiomatic |
| **Toolbar state** | Tiptap's `editor.isActive()` is simpler than Lexical's `$getSelection` + `getFormat()` |
| **Tables** | Lexical's table is more capable (nested); Tiptap's is easier to set up |
| **Pro features** | Tiptap gates drag-handle and math behind a paid plan; Lexical is fully open-source |
| **Bundle size** | Lexical is more modular; Tiptap's StarterKit pulls in ProseMirror |
| **Performance** | Lexical's reconciler is faster for large docs; Tiptap relies on ProseMirror's |

---

## Notes on Tiptap Pro

Several features in the comparison matrix are gated behind **Tiptap Pro** (paid subscription):
- `@tiptap/extension-drag-handle` — drag block reordering
- `@tiptap/extension-mathematics` — KaTeX equations
- `@tiptap/extension-details` — collapsible containers (Pro version)
- `@tiptap/extension-table-of-contents` — live TOC

For this case study, we will use **community or custom implementations** for these features so the comparison is fair (Tiptap free-tier vs Lexical open-source).

---

## File Structure (Target)

```
src/
  editors/
    tiptap/
      TiptapEditor.tsx           ← root component
      extensions/                ← custom Tiptap extensions
      components/
        Toolbar.tsx
        BubbleMenu.tsx
        FloatingMenu.tsx
        SlashMenu.tsx
        LinkPopover.tsx
        TableMenu.tsx
    lexical/
      LexicalEditor.tsx          ← root component
      nodes/                     ← custom Lexical nodes
      plugins/                   ← Lexical plugins
      components/
        Toolbar.tsx
        FloatingToolbar.tsx
        SlashMenu.tsx
        LinkPopover.tsx
  pages/
    Home.tsx                     ← tab switcher, mounts both
  components/
    ui/                          ← shadcn components (shared)
```
