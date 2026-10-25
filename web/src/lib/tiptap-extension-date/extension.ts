import { Node, mergeAttributes } from '@tiptap/core'

export interface DateChipOptions {
  HTMLAttributes: Record<string, unknown>
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    dateChip: {
      insertDateChip: (date: string) => ReturnType
    }
  }
}

export const DateChip = Node.create<DateChipOptions>({
  name: 'dateChip',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addOptions() {
    return { HTMLAttributes: {} }
  },

  addAttributes() {
    return {
      date: {
        default: null,
        parseHTML: (el) => el.getAttribute('data-date'),
        renderHTML: (attrs) => ({ 'data-date': attrs.date }),
      },
    }
  },

  parseHTML() {
    return [{ tag: 'span[data-type="date-chip"]' }]
  },

  renderHTML({ HTMLAttributes }) {
    const date = HTMLAttributes['data-date'] as string | null
    const formatted = date ? formatDate(date) : ''
    return [
      'span',
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
        'data-type': 'date-chip',
        class: 'date-chip',
      }),
      formatted,
    ]
  },

  addCommands() {
    return {
      insertDateChip:
        (date: string) =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name, attrs: { date } })
        },
    }
  },
})

export function formatDate(iso: string): string {
  const [year, month, day] = iso.split('-').map(Number)
  return new Date(year, month - 1, day).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  })
}
