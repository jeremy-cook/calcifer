import type { SuggestionOptions } from '@tiptap/suggestion'
import { useEntityStore } from '~/model/store'
import { STRUCTURES, hasUniqueNames, isMentionable, type StructureType } from '~/model/structures'
import { createSuggestionPopup } from '~/editors/tiptap/components/suggestionPopup'
import { MentionMenu } from './MentionMenu'
import { parseQuery } from './parseQuery'
import { CREATE_ITEM_ID, type EntitySuggestionItem } from './types'

interface SuggestionConfig {
  char: '@' | '#'
  structureFilter?: StructureType
}

const MAX_RESULTS = 8

export function makeSuggestion(config: SuggestionConfig): Omit<SuggestionOptions<EntitySuggestionItem>, 'editor'> {
  return {
    char: config.char,

    items: ({ query }) => {
      const { structureType, term } = config.structureFilter
        ? { structureType: config.structureFilter, term: query }
        : parseQuery(query)
      // Bare `@` (no narrowing, no explicit filter): only Structures with mentionable !== false.
      const filterByMentionable = config.char === '@' && !config.structureFilter && structureType === null
      const q = term.toLowerCase()
      const entities = useEntityStore.getState().entities
      const results: EntitySuggestionItem[] = []
      let exactMatch = false
      for (const entity of Object.values(entities)) {
        if (structureType && entity.structureType !== structureType) continue
        if (filterByMentionable && !isMentionable(entity.structureType)) continue
        if (q && !entity.title.toLowerCase().includes(q)) continue
        if (entity.title.toLowerCase() === q) exactMatch = true
        if (results.length < MAX_RESULTS) {
          const meta = STRUCTURES[entity.structureType as StructureType]
          results.push({
            id: entity.id,
            label: entity.title,
            structureType: entity.structureType,
            color: meta?.color ?? 'var(--muted-foreground)',
          })
        }
        // Keep scanning past the cap only to settle exactMatch (it gates the
        // create-on-miss item below); once it's true there's nothing more to learn.
        if (results.length >= MAX_RESULTS && exactMatch) break
      }

      const createStructureType = resolveCreateStructureType(structureType)
      if (createStructureType && term.length > 0 && !exactMatch) {
        const createMeta = STRUCTURES[createStructureType]
        results.push({
          id: CREATE_ITEM_ID,
          label: term,
          structureType: createStructureType,
          color: createMeta.color,
          isCreate: true,
          createLabel: `Create new ${createMeta.name}: ${term}`,
        })
      }
      return results
    },

    command: ({ editor, range, props }) => {
      const item = props.isCreate
        ? createEntityForMention(props)
        : { id: props.id, label: props.label, structureType: props.structureType }

      editor
        .chain()
        .focus()
        .deleteRange(range)
        .insertContent([
          {
            type: config.char === '#' ? 'hashtag' : 'mention',
            attrs: {
              id: item.id,
              label: item.label,
              structureType: item.structureType,
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

function resolveCreateStructureType(narrowedStructureType: StructureType | null): StructureType | undefined {
  if (!narrowedStructureType) return undefined
  const meta = STRUCTURES[narrowedStructureType] as { creatable?: boolean }
  return meta.creatable === false ? undefined : narrowedStructureType
}

function createEntityForMention(props: EntitySuggestionItem) {
  const structureType = props.structureType as Exclude<StructureType, 'DailyNote'>
  const store = useEntityStore.getState()
  // For uniqueNames Structures (Tags), reuse an existing case-insensitive match
  // instead of spawning a duplicate — guards the casing/race the items() exact-
  // match check can't catch on its own.
  if (hasUniqueNames(structureType)) {
    const term = props.label.toLowerCase()
    const existing = Object.values(store.entities).find(
      (e) => e.structureType === structureType && e.title.toLowerCase() === term,
    )
    if (existing) {
      return { id: existing.id, label: existing.title, structureType: existing.structureType }
    }
  }
  const entity = store.createEntity(structureType, props.label)
  return { id: entity.id, label: entity.title, structureType: entity.structureType }
}
