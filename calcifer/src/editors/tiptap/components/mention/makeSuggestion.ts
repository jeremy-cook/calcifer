import type { SuggestionOptions } from '@tiptap/suggestion'
import {
  getEntitiesSnapshot,
  getOrCreateEntityForMention,
  type CreatableStructureType,
} from '~/model/store'
import { STRUCTURES, isMentionable, type StructureType } from '~/model/structures'
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
      const entities = getEntitiesSnapshot()
      const results: EntitySuggestionItem[] = []
      let exactMatch = false
      for (const entity of entities) {
        if (structureType && entity.structureType !== structureType) continue
        if (filterByMentionable && !isMentionable(entity.structureType)) continue
        if (q && !entity.name.toLowerCase().includes(q)) continue
        if (entity.name.toLowerCase() === q) exactMatch = true
        if (results.length < MAX_RESULTS) {
          const meta = STRUCTURES[entity.structureType as StructureType]
          results.push({
            id: entity.id,
            label: entity.name,
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
      void resolveMentionItem(props).then((item) => insertMention(editor, range, config.char, item))
    },

    render: createSuggestionPopup(MentionMenu),
  }
}

interface MentionItem {
  id: string
  label: string
  structureType: string
}

async function resolveMentionItem(props: EntitySuggestionItem): Promise<MentionItem> {
  if (props.isCreate) {
    // Server-authoritative get-or-create: browser and MCP agent resolve to one id.
    const created = await getOrCreateEntityForMention(props.structureType as CreatableStructureType, props.label)
    return { id: created.id, label: created.name, structureType: created.structureType }
  }
  return { id: props.id, label: props.label, structureType: props.structureType }
}

function insertMention(
  editor: Parameters<NonNullable<SuggestionOptions<EntitySuggestionItem>['command']>>[0]['editor'],
  range: Parameters<NonNullable<SuggestionOptions<EntitySuggestionItem>['command']>>[0]['range'],
  char: '@' | '#',
  item: MentionItem,
) {
  editor
    .chain()
    .focus()
    .deleteRange(range)
    .insertContent([
      {
        type: char === '#' ? 'hashtag' : 'mention',
        attrs: {
          id: item.id,
          label: item.label,
          structureType: item.structureType,
          char,
        },
      },
      { type: 'text', text: ' ' },
    ])
    .run()
}

function resolveCreateStructureType(narrowedStructureType: StructureType | null): StructureType | undefined {
  if (!narrowedStructureType) return undefined
  const meta = STRUCTURES[narrowedStructureType] as { creatable?: boolean }
  return meta.creatable === false ? undefined : narrowedStructureType
}
