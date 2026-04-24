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

const MENTION_TYPES = new Set(['mention', 'hashtag'])

export function extractMentions(doc: JSONContent): ExtractedMention[] {
  const seen = new Map<string, ExtractedMention>()
  const walk = (node: JSONContent) => {
    if (node.type && MENTION_TYPES.has(node.type)) {
      const id = node.attrs?.id as string | undefined
      const structureType = node.attrs?.structureType as string | undefined
      if (id && structureType && !seen.has(id)) {
        seen.set(id, { id, structureType })
      }
    }
    if (node.content) {
      for (const child of node.content) walk(child)
    }
  }
  walk(doc)
  return [...seen.values()]
}

export function syncLinksFromDoc(entityId: string, doc: JSONContent): void {
  const state = useEntityStore.getState()
  const entity = state.entities[entityId]
  if (!entity) return

  const mentions = extractMentions(doc)
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
  const sameSize = prevIds.size === mentionIds.size
  const sameMembers = sameSize && [...mentionIds].every((id) => prevIds.has(id))
  if (sameMembers) return

  state.setLinks(entityId, nextLinks)
}
