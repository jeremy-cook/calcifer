import { useMemo } from 'react'
import { useAllEntities } from '~/model/store'
import { getStructures } from '~/model/structures'
import type { Entity, LinkRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { timestampMsOrZero } from '~/model/dates'

export interface Backlink {
  entityId: string
  structureType: string
  name: string
  mostRecentAt: Date
}

function mostRecentLinkMillis(links: readonly LinkRef[], targetId: string): number {
  let max = 0
  for (const link of links) {
    if (link.target?.id !== targetId) continue
    const ms = timestampMsOrZero(link.createdAt)
    if (ms > max) max = ms
  }
  return max
}

// Groups follow registry order; unknown structures sort last.
function compareBacklinks(a: Backlink, b: Backlink, structureIndex: Record<string, number>): number {
  const ai = structureIndex[a.structureType] ?? Number.MAX_SAFE_INTEGER
  const bi = structureIndex[b.structureType] ?? Number.MAX_SAFE_INTEGER
  if (ai !== bi) return ai - bi
  return b.mostRecentAt.getTime() - a.mostRecentAt.getTime()
}

export function selectBacklinks(entities: Entity[], targetEntityId: string): Backlink[] {
  const results: Backlink[] = []
  for (const source of entities) {
    // Self-links are excluded, matching the server's EntityService.ListBacklinks.
    if (source.id === targetEntityId) continue
    const ms = mostRecentLinkMillis(source.links, targetEntityId)
    if (ms === 0) continue
    results.push({
      entityId: source.id,
      structureType: source.structureType,
      name: source.name,
      mostRecentAt: new Date(ms),
    })
  }
  const structureIndex = Object.fromEntries(getStructures().map((s, i) => [s.type, i]))
  results.sort((a, b) => compareBacklinks(a, b, structureIndex))
  return results
}

export function useBacklinks(targetEntityId: string): Backlink[] {
  const entities = useAllEntities()
  return useMemo(() => selectBacklinks(entities, targetEntityId), [entities, targetEntityId])
}
