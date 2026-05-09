import type { JSONContent } from '@tiptap/core'
import { create as createMessage } from '@bufbuild/protobuf'
import { timestampNow } from '@bufbuild/protobuf/wkt'
import {
  EntityRefSchema,
  LinkRefSchema,
  type LinkRef,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { useEntityStore } from '~/model/store'

interface ExtractedMention {
  id: string
  structureType: string
}

interface DocReferences {
  entities: ExtractedMention[]
  dates: string[]
}

const MENTION_TYPES = new Set(['mention', 'hashtag'])

export function extractDocReferences(doc: JSONContent): DocReferences {
  const seenEntities = new Map<string, ExtractedMention>()
  const seenDates = new Set<string>()
  const walk = (node: JSONContent) => {
    if (node.type && MENTION_TYPES.has(node.type)) {
      const id = node.attrs?.id as string | undefined
      const structureType = node.attrs?.structureType as string | undefined
      if (id && structureType && !seenEntities.has(id)) {
        seenEntities.set(id, { id, structureType })
      }
    } else if (node.type === 'dateChip') {
      const iso = node.attrs?.date as string | undefined
      if (iso) seenDates.add(iso)
    }
    if (node.content) {
      for (const child of node.content) walk(child)
    }
  }
  walk(doc)
  return { entities: [...seenEntities.values()], dates: [...seenDates] }
}

export function syncLinksFromDoc(entityId: string, doc: JSONContent): void {
  const state = useEntityStore.getState()
  const entity = state.entities[entityId]
  if (!entity) return

  const { entities: mentions, dates } = extractDocReferences(doc)

  const mentionIds = new Set(mentions.map((m) => m.id))
  const existingByTargetId = new Map(
    entity.links.filter((l) => l.target).map((l) => [l.target!.id, l] as const),
  )

  const now = timestampNow()
  const nextLinks: LinkRef[] = mentions.map((m) => {
    const existing = existingByTargetId.get(m.id)
    if (existing) return existing
    return createMessage(LinkRefSchema, {
      id: crypto.randomUUID(),
      target: createMessage(EntityRefSchema, {
        id: m.id,
        structureType: m.structureType,
      }),
      createdAt: now,
    })
  })

  const prevIds = new Set(existingByTargetId.keys())
  const linksChanged =
    prevIds.size !== mentionIds.size || [...mentionIds].some((id) => !prevIds.has(id))
  if (linksChanged) state.setLinks(entityId, nextLinks)

  const prevDates = new Set(entity.referencedDates)
  const nextDates = new Set(dates)
  const datesChanged =
    prevDates.size !== nextDates.size || [...nextDates].some((d) => !prevDates.has(d))
  if (datesChanged) state.setReferencedDates(entityId, dates)
}
