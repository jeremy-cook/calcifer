import type { JSONContent } from '@tiptap/core'
import { create as createMessage } from '@bufbuild/protobuf'
import { timestampNow } from '@bufbuild/protobuf/wkt'
import {
  EntityRefSchema,
  LinkRefSchema,
  RichTextRefSchema,
  type LinkRef,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { useEntityStore } from '~/model/store'
import { useRichTextStore } from '~/model/richtext'
import { STRUCTURES, type StructureType } from '~/model/structures'

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

export function syncLinksFromDoc(entityId: string, propertyId: string, doc: JSONContent): void {
  const state = useEntityStore.getState()
  const entity = state.entities[entityId]
  if (!entity) return

  const { entities: mentions, dates: currentDates } = extractDocReferences(doc)

  // Links: reconcile only the subset this property owns. Links sourced from
  // other properties pass through untouched. Legacy links with an empty
  // source_property_id are claimed by the saving property — historically every
  // link came from `content`, the only richtext property ever shipped.
  const isClaimed = (l: LinkRef) => l.sourcePropertyId === '' || l.sourcePropertyId === propertyId
  const foreignLinks = entity.links.filter((l) => !isClaimed(l))
  const claimedLinks = entity.links.filter(isClaimed)
  const claimedByTarget = new Map(claimedLinks.filter((l) => l.target).map((l) => [l.target!.id, l] as const))

  const now = timestampNow()
  const nextClaimed: LinkRef[] = mentions.map((m) => {
    const existing = claimedByTarget.get(m.id)
    if (existing && existing.sourcePropertyId === propertyId) return existing
    // New mention, or a legacy ('') link being restamped with its origin property.
    return createMessage(LinkRefSchema, {
      id: existing?.id ?? crypto.randomUUID(),
      target: createMessage(EntityRefSchema, { id: m.id, structureType: m.structureType }),
      sourcePropertyId: propertyId,
      createdAt: existing?.createdAt ?? now,
    })
  })

  const prevTargets = new Set(claimedByTarget.keys())
  const nextTargets = new Set(mentions.map((m) => m.id))
  const targetsChanged = prevTargets.size !== nextTargets.size || [...nextTargets].some((id) => !prevTargets.has(id))
  // Restamp once even when targets are unchanged, to migrate legacy '' provenance.
  const needsRestamp = claimedLinks.some((l) => l.sourcePropertyId === '')
  if (targetsChanged || needsRestamp) {
    state.setLinks(entityId, [...foreignLinks, ...nextClaimed])
  }

  // Referenced dates are entity-scoped with no per-property provenance, so
  // recompute the union across every richtext doc each save. The current doc is
  // authoritative for `propertyId`; siblings are read from the store (already
  // fresh — putRichText runs before this in the autosave callback).
  const dates = collectReferencedDates(entity.structureType, entityId, propertyId, currentDates)
  const prevDates = new Set(entity.referencedDates)
  const nextDates = new Set(dates)
  const datesChanged = prevDates.size !== nextDates.size || [...nextDates].some((d) => !prevDates.has(d))
  if (datesChanged) state.setReferencedDates(entityId, dates)
}

function collectReferencedDates(
  structureType: string,
  entityId: string,
  currentPropertyId: string,
  currentDates: string[],
): string[] {
  const structure = STRUCTURES[structureType as StructureType]
  const richTextStore = useRichTextStore.getState()
  const dates = new Set(currentDates)
  for (const def of structure?.properties ?? []) {
    if (def.type !== 'richtext' || def.id === currentPropertyId) continue
    const ref = createMessage(RichTextRefSchema, { entityId, propertyId: def.id })
    const stored = richTextStore.getRichText(ref)?.doc
    if (!stored) continue
    try {
      for (const iso of extractDocReferences(JSON.parse(stored) as JSONContent).dates) {
        dates.add(iso)
      }
    } catch {
      // skip unparseable sibling doc
    }
  }
  return [...dates]
}
