// Minimal TipTap/ProseMirror JSON node shape. The server only inspects
// mention/hashtag/dateChip nodes (anywhere in the tree) to derive the graph,
// but the doc must be valid TipTap JSON for the editor to render it.
export interface TTNode {
  type: string
  attrs?: Record<string, unknown>
  content?: TTNode[]
  text?: string
}

// Resolves a (structureType, name) to a canonical entity id (server ResolveByName).
export type Resolver = (structureType: string, name: string) => Promise<{ id: string; name: string }>
