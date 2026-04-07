import type { Editor, Range } from '@tiptap/core'

export type SlashCommandItem = {
  title: string
  subtitle: string
  group: string
  searchTerms?: string[]
  command: (props: { editor: Editor; range: Range }) => void
}

export const SLASH_ITEMS: SlashCommandItem[] = [
  // Text
  {
    title: 'Paragraph',
    subtitle: 'Plain text',
    group: 'Text',
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setParagraph().run(),
  },
  {
    title: 'Quote',
    subtitle: 'Block quotation',
    group: 'Text',
    searchTerms: ['blockquote'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleBlockquote().run(),
  },
  // Headings
  {
    title: 'Heading 1',
    subtitle: 'Large section heading',
    group: 'Heading',
    searchTerms: ['h1', 'title'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHeading({ level: 1 }).run(),
  },
  {
    title: 'Heading 2',
    subtitle: 'Medium section heading',
    group: 'Heading',
    searchTerms: ['h2'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHeading({ level: 2 }).run(),
  },
  {
    title: 'Heading 3',
    subtitle: 'Small section heading',
    group: 'Heading',
    searchTerms: ['h3'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHeading({ level: 3 }).run(),
  },
  // Lists
  {
    title: 'Bullet List',
    subtitle: 'Unordered list',
    group: 'List',
    searchTerms: ['ul', 'unordered'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleBulletList().run(),
  },
  {
    title: 'Numbered List',
    subtitle: 'Ordered list',
    group: 'List',
    searchTerms: ['ol', 'ordered'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleOrderedList().run(),
  },
  {
    title: 'Task List',
    subtitle: 'Checklist with checkboxes',
    group: 'List',
    searchTerms: ['todo', 'checklist', 'checkbox'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleTaskList().run(),
  },
  // Table
  {
    title: 'Table',
    subtitle: 'Insert a 3×3 table',
    group: 'Table',
    command: ({ editor, range }) =>
      editor.chain().focus().deleteRange(range).insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
  },
  // Media
  {
    title: 'Code Block',
    subtitle: 'Fenced code with syntax highlighting',
    group: 'Media',
    searchTerms: ['pre', 'fence', 'code'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleCodeBlock().run(),
  },
  {
    title: 'Image',
    subtitle: 'Insert image from URL',
    group: 'Media',
    command: ({ editor, range }) => {
      const url = window.prompt('Image URL')
      if (url) editor.chain().focus().deleteRange(range).setImage({ src: url }).run()
    },
  },
  {
    title: 'Divider',
    subtitle: 'Horizontal rule',
    group: 'Media',
    searchTerms: ['hr', 'rule', 'separator'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHorizontalRule().run(),
  },
]
