import { useMemo } from 'react'
import { keepPreviousData, useQuery } from '@tanstack/react-query'
import type { MatchRange } from 'calcifer/v1/services_pb'
import { qk, searchClient } from '~/model/api'
import { timestampMsOrZero } from '~/model/dates'
import { useAllEntities, type Entity } from '~/model/store'

// The search palette's two sources (ADR 9, 2026-11-01 note): titles come from the
// replica as you type; note text and meaning come from the server's Search.

const TITLE_LIMIT = 5
const TEXT_LIMIT = 8
// Shorter queries match too much text to be worth a round trip.
export const MIN_TEXT_QUERY = 2

// Entities whose name contains `query`, names that start with it first, then the
// most recently edited.
export function useTitleMatches(query: string): Entity[] {
  const entities = useAllEntities()
  return useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return []
    return entities
      .filter((e) => e.name.toLowerCase().includes(q))
      .sort((a, b) => {
        const aStarts = a.name.toLowerCase().startsWith(q) ? 0 : 1
        const bStarts = b.name.toLowerCase().startsWith(q) ? 0 : 1
        return aStarts - bStarts || timestampMsOrZero(b.updatedAt) - timestampMsOrZero(a.updatedAt)
      })
      .slice(0, TITLE_LIMIT)
  }, [entities, query])
}

export interface TextHit {
  entity: Entity
  // Plain text around the best match; empty for a hit found only by meaning.
  snippet: string
  matches: readonly MatchRange[]
}

// The server's hybrid search over names and bodies. The caller debounces `query`.
// While a new query is in flight the previous hits stay, so the list doesn't flash.
export function useTextSearch(query: string) {
  const q = query.trim()
  return useQuery({
    queryKey: qk.search(q),
    queryFn: async (): Promise<TextHit[]> => {
      const { hits } = await searchClient.search({ query: q, limit: TEXT_LIMIT })
      return hits
        .filter((hit) => hit.entity !== undefined)
        .map((hit) => ({ entity: hit.entity!, snippet: hit.snippet, matches: hit.matches }))
    },
    enabled: q.length >= MIN_TEXT_QUERY,
    placeholderData: keepPreviousData,
    staleTime: 10_000,
  })
}
