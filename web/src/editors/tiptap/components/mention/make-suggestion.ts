import type { SuggestionOptions } from '@tiptap/suggestion'
import { findMentionTargets } from '~/model/mentions'
import { getOrCreateEntityForMention } from '~/model/store'
import { getStructure, structurePresentation } from '~/model/structures'
import { createSuggestionPopup } from '~/editors/tiptap/components/suggestion-popup'
import { MentionMenu } from './MentionMenu'
import { parseQuery } from './parse-query'
import { CREATE_ITEM_ID, type EntitySuggestionItem } from './types'

interface SuggestionConfig {
  char: '@' | '#'
  structureFilter?: string
}

const MAX_RESULTS = 8

export function makeSuggestion(config: SuggestionConfig): Omit<SuggestionOptions<EntitySuggestionItem>, 'editor'> {
  let createInFlight = false

  return {
    char: config.char,

    items: ({ query }) => {
      const { structureType, term } = config.structureFilter
        ? { structureType: config.structureFilter, term: query }
        : parseQuery(query)
      // Bare `@` (no narrowing, no explicit filter): only Structures with mentionable !== false.
      const filterByMentionable = config.char === '@' && !config.structureFilter && structureType === null
      const { entities, exactMatch } = findMentionTargets({
        term,
        structureType,
        mentionableOnly: filterByMentionable,
        limit: MAX_RESULTS,
      })
      const results: EntitySuggestionItem[] = entities.map((entity) => ({
        id: entity.id,
        label: entity.name,
        structureType: entity.structureType,
        color: structurePresentation(entity.structureType).color,
      }))

      const createStructureType = resolveCreateStructureType(structureType)
      if (createStructureType && term.length > 0 && !exactMatch) {
        const createName = getStructure(createStructureType)?.name ?? createStructureType
        results.push({
          id: CREATE_ITEM_ID,
          label: term,
          structureType: createStructureType,
          color: structurePresentation(createStructureType).color,
          isCreate: true,
          createLabel: `Create new ${createName}: ${term}`,
        })
      }
      return results
    },

    command: ({ editor, range, props }) => {
      // The suggestion stays open until the mention is inserted, so a second Enter or
      // click can land while a create's ResolveEntity is pending. Ignore it (I-43).
      if (createInFlight) return
      createInFlight = props.isCreate === true
      void resolveMentionItem(props)
        .then((item) => insertMention(editor, range, config.char, item))
        .finally(() => {
          if (props.isCreate) createInFlight = false
        })
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
    const created = await getOrCreateEntityForMention(props.structureType, props.label)
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

function resolveCreateStructureType(narrowedStructureType: string | null): string | undefined {
  if (!narrowedStructureType) return undefined
  return getStructure(narrowedStructureType)?.creatable === false ? undefined : narrowedStructureType
}
