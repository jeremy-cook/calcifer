import { STRUCTURE_LIST, type StructureType } from '~/model/structures'

export interface ParsedQuery {
  structureType: StructureType | null
  term: string
}

export function parseQuery(query: string): ParsedQuery {
  const slash = query.indexOf('/')
  if (slash === -1) return { structureType: null, term: query }

  const prefix = query.slice(0, slash).toLowerCase()
  const term = query.slice(slash + 1)
  const match = STRUCTURE_LIST.find((s) => s.name.toLowerCase() === prefix)
  if (!match) return { structureType: null, term: query }
  return { structureType: match.type as StructureType, term }
}
