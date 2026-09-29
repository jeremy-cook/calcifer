// Minimal TipTap/ProseMirror JSON node shape. The server only inspects
// mention/hashtag/dateChip nodes (anywhere in the tree) to derive the graph,
// but the doc must be valid TipTap JSON for the editor to render it.
export interface TTNode {
  type: string
  attrs?: Record<string, unknown>
  content?: TTNode[]
  text?: string
  marks?: { type: string }[]
}

// Resolves a (structureType, name) to a canonical entity id (server Resolve by name).
// null means there's no such entity and none may be created; the chip becomes text.
export type Resolver = (structureType: string, name: string) => Promise<{ id: string; name: string } | null>

// A chip target's current identity, looked up by id when writing markdown.
export interface ChipTarget {
  name: string
  structureType: string
}

// The structure a bare [[Name]] refers to. Any other structure is written
// [[Structure/Name]].
export const DEFAULT_MENTION_STRUCTURE = 'Note'

// Splits "Structure/Name" when Structure is exactly a registry structure type
// and Name is non-empty; otherwise the whole text is a Note name.
export function splitStructurePrefix(text: string, structureTypes: ReadonlySet<string>): ChipTarget {
  const slash = text.indexOf('/')
  if (slash > 0) {
    const structureType = text.slice(0, slash)
    const name = text.slice(slash + 1).trim()
    if (name && structureTypes.has(structureType)) return { structureType, name }
  }
  return { structureType: DEFAULT_MENTION_STRUCTURE, name: text }
}
