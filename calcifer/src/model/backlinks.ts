import { useMemo } from 'react'
import { useEntityStore } from '~/model/store'
import { STRUCTURE_LIST } from '~/model/structures'
import type { Entity, LinkRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import type { Timestamp } from '@bufbuild/protobuf/wkt'

export interface Backlink {
  entityId: string
  structureType: string
  title: string
  mostRecentAt: Date
}

const STRUCTURE_INDEX: Record<string, number> = Object.fromEntries(
  STRUCTURE_LIST.map((s, i) => [s.type, i]),
)

function timestampMillis(ts: Timestamp | undefined): number {
  if (!ts) return 0
  return Number(ts.seconds) * 1000 + ts.nanos / 1_000_000
}

function mostRecentLinkMillis(links: readonly LinkRef[], targetId: string): number {
  let max = 0
  for (const link of links) {
    if (link.target?.id !== targetId) continue
    const ms = timestampMillis(link.createdAt)
    if (ms > max) max = ms
  }
  return max
}

function compareBacklinks(a: Backlink, b: Backlink): number {
  const ai = STRUCTURE_INDEX[a.structureType] ?? Number.MAX_SAFE_INTEGER
  const bi = STRUCTURE_INDEX[b.structureType] ?? Number.MAX_SAFE_INTEGER
  if (ai !== bi) return ai - bi
  return b.mostRecentAt.getTime() - a.mostRecentAt.getTime()
}

export function selectBacklinks(
  entities: Record<string, Entity>,
  targetEntityId: string,
): Backlink[] {
  const results: Backlink[] = []
  for (const source of Object.values(entities)) {
    if (source.id === targetEntityId) continue
    const ms = mostRecentLinkMillis(source.links, targetEntityId)
    if (ms === 0) continue
    results.push({
      entityId: source.id,
      structureType: source.structureType,
      title: source.title,
      mostRecentAt: new Date(ms),
    })
  }
  results.sort(compareBacklinks)
  return results
}

export function useBacklinks(targetEntityId: string): Backlink[] {
  const entities = useEntityStore((s) => s.entities)
  return useMemo(() => selectBacklinks(entities, targetEntityId), [entities, targetEntityId])
}
