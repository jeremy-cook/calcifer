import type { SuggestionOptions } from '@tiptap/suggestion'
import { useEntityStore } from '~/model/store'
import { STRUCTURES, type StructureType } from '~/model/structures'
import { createSuggestionPopup } from '~/editors/tiptap/components/suggestionPopup'
import { MentionMenu } from './MentionMenu'
import { parseQuery } from './parseQuery'
import type { EntitySuggestionItem } from './types'

interface SuggestionConfig {
  char: '@' | '#'
  structureFilter?: StructureType
}

const MAX_RESULTS = 8

export function makeSuggestion(
  config: SuggestionConfig,
): Omit<SuggestionOptions<EntitySuggestionItem>, 'editor'> {
  return {
    char: config.char,

    items: ({ query }) => {
      const { structureType, term } = config.structureFilter
        ? { structureType: config.structureFilter, term: query }
        : parseQuery(query)
      // For bare `@` (no slash narrowing, no explicit filter): exclude Tags —
      // use `#` or `@tag/...` to reach them.
      const excludeTags = config.char === '@' && !config.structureFilter && structureType === null
      const q = term.toLowerCase()
      const entities = useEntityStore.getState().entities
      const results: EntitySuggestionItem[] = []
      for (const entity of Object.values(entities)) {
        if (structureType && entity.structureType !== structureType) continue
        if (excludeTags && entity.structureType === 'Tag') continue
        if (q && !entity.title.toLowerCase().includes(q)) continue
        const meta = STRUCTURES[entity.structureType as StructureType]
        results.push({
          id: entity.id,
          label: entity.title,
          structureType: entity.structureType,
          color: meta?.color ?? 'var(--muted-foreground)',
        })
        if (results.length >= MAX_RESULTS) break
      }
      return results
    },

    command: ({ editor, range, props }) => {
      editor
        .chain()
        .focus()
        .deleteRange(range)
        .insertContent([
          {
            type: config.char === '#' ? 'hashtag' : 'mention',
            attrs: {
              id: props.id,
              label: props.label,
              structureType: props.structureType,
              char: config.char,
            },
          },
          { type: 'text', text: ' ' },
        ])
        .run()
    },

    render: createSuggestionPopup(MentionMenu),
  }
}
