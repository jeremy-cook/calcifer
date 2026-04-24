import type { SuggestionOptions } from '@tiptap/suggestion'
import { SLASH_ITEMS, type SlashCommandItem } from '~/lib/tiptap-extension-slash-command'
import { createSuggestionPopup } from '~/editors/tiptap/components/suggestionPopup'
import { SlashCommandMenu } from './SlashCommandMenu'

export const slashSuggestion: Omit<SuggestionOptions<SlashCommandItem>, 'editor'> = {
  char: '/',

  items: ({ query }) => {
    if (!query) return SLASH_ITEMS
    const q = query.toLowerCase()
    return SLASH_ITEMS.filter(
      (item) =>
        item.title.toLowerCase().includes(q) ||
        item.group.toLowerCase().includes(q) ||
        item.searchTerms?.some((t) => t.includes(q)),
    ).slice(0, 12)
  },

  render: createSuggestionPopup(SlashCommandMenu),
}
