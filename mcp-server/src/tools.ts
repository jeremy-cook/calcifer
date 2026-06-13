// Knowledge operations the MCP tools delegate to. Everything goes through the
// tonic core: agents author markdown, the server derives the link graph.
import { Code, ConnectError } from '@connectrpc/connect'
import { entityClient, richTextClient } from './calciferClient.js'
import { toTipTap } from './markdown/parse.js'
import { fromTipTap } from './markdown/serialize.js'
import type { TTNode } from './markdown/types.js'

// [[wikilinks]] / #tags resolve to canonical entities (get-or-create) server-side.
const resolver = async (structureType: string, name: string) => {
  const r = await entityClient.resolveByName({ structureType, name, createIfMissing: true })
  return { id: r.entity!.id, name: r.entity!.name }
}

const contentRef = (entityId: string) => ({ entityId, propertyId: 'content' })
const emptyDoc = (): TTNode => ({ type: 'doc', content: [] })

function isNotFound(e: unknown): boolean {
  return e instanceof ConnectError && e.code === Code.NotFound
}

async function getDoc(entityId: string): Promise<TTNode> {
  try {
    const rt = await richTextClient.get(contentRef(entityId))
    return rt.doc ? (JSON.parse(rt.doc) as TTNode) : emptyDoc()
  } catch (e) {
    if (isNotFound(e)) return emptyDoc()
    throw e
  }
}

async function backlinkNames(targetId: string): Promise<string[]> {
  const res = await entityClient.list({ structureType: '' })
  return res.entities.filter((e) => e.links.some((l) => l.target?.id === targetId)).map((e) => e.name)
}

export async function createNote(name: string, markdown: string): Promise<string> {
  const { id } = await resolver('Note', name)
  const doc = await toTipTap(markdown, resolver)
  await richTextClient.put({ ref: contentRef(id), doc: JSON.stringify(doc) })
  const ent = await entityClient.get({ id })
  const linked = ent.links.map((l) => l.target?.structureType).join(', ') || 'none'
  return `Saved note "${ent.name}" (${id}). Derived links: ${ent.links.length} [${linked}]; dates: ${ent.referencedDates.length}.`
}

export async function appendToNote(name: string, markdown: string): Promise<string> {
  const { id } = await resolver('Note', name)
  const existing = await getDoc(id)
  const addition = await toTipTap(markdown, resolver)
  const merged: TTNode = { type: 'doc', content: [...(existing.content ?? []), ...(addition.content ?? [])] }
  await richTextClient.put({ ref: contentRef(id), doc: JSON.stringify(merged) })
  const ent = await entityClient.get({ id })
  return `Appended to "${ent.name}" (${id}); now ${ent.links.length} link(s).`
}

export async function getNote(name: string): Promise<string> {
  let entityId: string
  let entityName: string
  try {
    const r = await entityClient.resolveByName({ structureType: 'Note', name, createIfMissing: false })
    entityId = r.entity!.id
    entityName = r.entity!.name
  } catch (e) {
    if (isNotFound(e)) return `No note named "${name}". Use search_notes or create_note.`
    throw e
  }
  const md = fromTipTap(await getDoc(entityId)).trim()
  const back = await backlinkNames(entityId)
  return `# ${entityName}\n\n${md || '(empty)'}\n\n---\nLinked from: ${back.join(', ') || '(nothing)'}`
}

export async function searchNotes(query: string, limit = 10): Promise<string> {
  const res = await entityClient.list({ structureType: '' })
  const q = query.toLowerCase()
  const hits = res.entities.filter((e) => e.name.toLowerCase().includes(q)).slice(0, limit)
  if (hits.length === 0) return `No matches for "${query}".`
  return hits.map((e) => `- [${e.structureType}] ${e.name}`).join('\n')
}

export async function getBacklinks(name: string): Promise<string> {
  const r = await entityClient.resolveByName({ structureType: 'Note', name, createIfMissing: false }).catch(() => null)
  if (!r?.entity) return `No note named "${name}".`
  const names = await backlinkNames(r.entity.id)
  return names.length ? names.map((n) => `- ${n}`).join('\n') : `Nothing links to "${name}" yet.`
}

export async function linkNotes(from: string, to: string): Promise<string> {
  // Links are content-derived, so "linking" = adding a [[to]] mention to `from`.
  await appendToNote(from, `Related: [[${to}]]`)
  return `Linked "${from}" -> "${to}".`
}

export function listStructures(): string {
  return [
    '- Note: a concept/topic note (the main building block; reference with [[Name]])',
    '- Tag: a label, written #tag in note content',
    '- DailyNote: a journal entry for a calendar day',
  ].join('\n')
}
