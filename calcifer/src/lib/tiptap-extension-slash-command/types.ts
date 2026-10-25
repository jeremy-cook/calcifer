import type { Editor, Range } from '@tiptap/core'
import type { Icon } from '@phosphor-icons/react'

export type SlashCommandItem = {
  title: string
  subtitle: string
  group: string
  icon?: Icon
  searchTerms?: string[]
  command: (props: { editor: Editor; range: Range }) => void
}
