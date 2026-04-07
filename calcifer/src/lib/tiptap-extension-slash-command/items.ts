import type { Editor, Range } from '@tiptap/core'
import type { Icon } from '@phosphor-icons/react'
import {
  CalendarBlankIcon,
  CalendarIcon,
  TextAlignLeftIcon,
  QuotesIcon,
  CaretRightIcon,
  TextHOneIcon,
  TextHTwoIcon,
  TextHThreeIcon,
  ListBulletsIcon,
  ListNumbersIcon,
  ListChecksIcon,
  TableIcon,
  CodeIcon,
  ImageIcon,
  MinusIcon,
} from '@phosphor-icons/react'
import { showDatePicker } from '~/editors/tiptap/components/date/showDatePicker'

export type SlashCommandItem = {
  title: string
  subtitle: string
  group: string
  icon?: Icon
  searchTerms?: string[]
  command: (props: { editor: Editor; range: Range }) => void
}

export const SLASH_ITEMS: SlashCommandItem[] = [
  // Inline
  {
    title: 'Today',
    subtitle: "Insert today's date",
    group: 'Inline',
    icon: CalendarBlankIcon,
    searchTerms: ['date', 'today', 'now'],
    command: ({ editor, range }) => {
      const today = new Date().toISOString().slice(0, 10)
      editor.chain().focus().deleteRange(range).insertDateChip(today).run()
    },
  },
  {
    title: 'Date',
    subtitle: 'Pick a date to insert',
    group: 'Inline',
    icon: CalendarIcon,
    searchTerms: ['date', 'calendar', 'pick'],
    command: ({ editor, range }) => {
      editor.chain().focus().deleteRange(range).run()
      showDatePicker(editor)
    },
  },
  // Text
  {
    title: 'Paragraph',
    subtitle: 'Plain text',
    group: 'Text',
    icon: TextAlignLeftIcon,
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setParagraph().run(),
  },
  {
    title: 'Quote',
    subtitle: 'Block quotation',
    group: 'Text',
    icon: QuotesIcon,
    searchTerms: ['blockquote'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleBlockquote().run(),
  },
  {
    title: 'Details',
    subtitle: 'Collapsible disclosure block',
    group: 'Text',
    icon: CaretRightIcon,
    searchTerms: ['disclosure', 'accordion', 'collapse', 'toggle', 'summary'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setDetails().run(),
  },
  // Headings
  {
    title: 'Heading 1',
    subtitle: 'Large section heading',
    group: 'Heading',
    icon: TextHOneIcon,
    searchTerms: ['h1', 'title'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHeading({ level: 1 }).run(),
  },
  {
    title: 'Heading 2',
    subtitle: 'Medium section heading',
    group: 'Heading',
    icon: TextHTwoIcon,
    searchTerms: ['h2'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHeading({ level: 2 }).run(),
  },
  {
    title: 'Heading 3',
    subtitle: 'Small section heading',
    group: 'Heading',
    icon: TextHThreeIcon,
    searchTerms: ['h3'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHeading({ level: 3 }).run(),
  },
  // Lists
  {
    title: 'Bullet List',
    subtitle: 'Unordered list',
    group: 'List',
    icon: ListBulletsIcon,
    searchTerms: ['ul', 'unordered'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleBulletList().run(),
  },
  {
    title: 'Numbered List',
    subtitle: 'Ordered list',
    group: 'List',
    icon: ListNumbersIcon,
    searchTerms: ['ol', 'ordered'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleOrderedList().run(),
  },
  {
    title: 'Task List',
    subtitle: 'Checklist with checkboxes',
    group: 'List',
    icon: ListChecksIcon,
    searchTerms: ['todo', 'checklist', 'checkbox'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleTaskList().run(),
  },
  // Table
  {
    title: 'Table',
    subtitle: 'Insert a 3×3 table',
    group: 'Table',
    icon: TableIcon,
    command: ({ editor, range }) =>
      editor.chain().focus().deleteRange(range).insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
  },
  // Media
  {
    title: 'Code Block',
    subtitle: 'Fenced code with syntax highlighting',
    group: 'Media',
    icon: CodeIcon,
    searchTerms: ['pre', 'fence', 'code'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).toggleCodeBlock().run(),
  },
  {
    title: 'Image',
    subtitle: 'Insert image from URL',
    group: 'Media',
    icon: ImageIcon,
    command: ({ editor, range }) => {
      const url = window.prompt('Image URL')
      if (url) editor.chain().focus().deleteRange(range).setImage({ src: url }).run()
    },
  },
  {
    title: 'Divider',
    subtitle: 'Horizontal rule',
    group: 'Media',
    icon: MinusIcon,
    searchTerms: ['hr', 'rule', 'separator'],
    command: ({ editor, range }) => editor.chain().focus().deleteRange(range).setHorizontalRule().run(),
  },
]
