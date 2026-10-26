import { getEntitiesSnapshot, type Entity } from '~/model/store'
import { isMentionable } from '~/model/structures'

export interface MentionQuery {
  // What the name must contain, case-insensitively; empty matches every name.
  term: string
  // Only entities of this structure, or any when null.
  structureType: string | null
  // Skip the structures that opt out of bare `@` mentions.
  mentionableOnly: boolean
  limit: number
}

export interface MentionMatches {
  // The first `limit` entities that match, in the replica's order.
  entities: Entity[]
  // Whether some entity's name equals the term exactly (past `limit` too); the
  // caller offers to create the name only when none does.
  exactMatch: boolean
}

// Reads the replica as it is now, without subscribing: it runs when a mention
// suggestion opens or its query changes, not during a render.
export function findMentionTargets({ term, structureType, mentionableOnly, limit }: MentionQuery): MentionMatches {
  const q = term.toLowerCase()
  const entities: Entity[] = []
  let exactMatch = false
  for (const entity of getEntitiesSnapshot()) {
    if (structureType && entity.structureType !== structureType) continue
    if (mentionableOnly && !isMentionable(entity.structureType)) continue
    const name = entity.name.toLowerCase()
    if (q && !name.includes(q)) continue
    if (name === q) exactMatch = true
    if (entities.length < limit) entities.push(entity)
    // Keep scanning past the cap only to settle exactMatch; once it's true there's
    // nothing more to learn.
    if (entities.length >= limit && exactMatch) break
  }
  return { entities, exactMatch }
}
