// Knowledge operations the MCP tools delegate to. Everything goes through the
// tonic core: agents author markdown, the server derives the link graph.
import { Code, ConnectError } from '@connectrpc/connect'
import { entityClient, richTextClient, searchClient, structureClient } from './calciferClient.js'
import { PropertyKind, type PropertyDef } from '@calcifer/proto/calcifer/v1/structures_pb'
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
  // Server-side backlink scan (SELECT ... WHERE target_id = ?), no client-side List.
  const res = await entityClient.listBacklinks({ id: targetId, structureType: '' })
  return res.entities.map((e) => e.name)
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

export type SearchMode = 'lexical' | 'semantic' | 'hybrid'

export async function searchNotes(
  query: string,
  limit = 10,
  mode: SearchMode = 'hybrid',
): Promise<string> {
  // Route by mode: `lexical` hits FTS5 (Search); `semantic`/`hybrid` hit the
  // embedding-backed Retrieve (pure vector vs RRF-fused with FTS5). Retrieve
  // degrades to lexical server-side when embeddings are disabled.
  const res =
    mode === 'lexical'
      ? await searchClient.search({ query, limit })
      : await searchClient.retrieve({ query, k: limit, hybrid: mode === 'hybrid' })
  if (res.hits.length === 0) return `No matches for "${query}".`
  return res.hits
    .map((h) => {
      const detail = h.snippet || `(score ${h.score.toFixed(3)})`
      return `- [${h.entity!.structureType}] ${h.entity!.name}: ${detail}`
    })
    .join('\n')
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

// Get-or-create the DailyNote for `date` (ISO "YYYY-MM-DD"), returning its id +
// name. CreateDailyNote is server-authoritative (one note per day); on a repeat
// call it returns already_exists, so we fall back to resolving by name.
async function resolveDailyNote(date: string): Promise<{ id: string; name: string }> {
  try {
    const e = await entityClient.createDailyNote({ date })
    return { id: e.id, name: e.name }
  } catch (err) {
    if (!(err instanceof ConnectError && err.code === Code.AlreadyExists)) throw err
    const list = await entityClient.list({ structureType: 'DailyNote' })
    const existing = list.entities.find((e) =>
      e.properties.some((p) => p.value?.value?.case === 'date' && p.value.value.value === date),
    )
    if (!existing) throw err
    return { id: existing.id, name: existing.name }
  }
}

export async function createDailyNote(date: string): Promise<string> {
  const { id, name } = await resolveDailyNote(date)
  return `Daily note "${name}" (${id}) ready for ${date}.`
}

export async function appendToDailyNote(date: string, markdown: string): Promise<string> {
  const { id, name } = await resolveDailyNote(date)
  const existing = await getDoc(id)
  const addition = await toTipTap(markdown, resolver)
  const merged: TTNode = { type: 'doc', content: [...(existing.content ?? []), ...(addition.content ?? [])] }
  await richTextClient.put({ ref: contentRef(id), doc: JSON.stringify(merged) })
  const ent = await entityClient.get({ id })
  return `Appended to daily note "${name}" (${id}); now ${ent.links.length} link(s).`
}

const KIND_NAMES: Record<PropertyKind, string> = {
  [PropertyKind.UNSPECIFIED]: 'unspecified',
  [PropertyKind.RICHTEXT]: 'richtext',
  [PropertyKind.TEXT]: 'text',
  [PropertyKind.NUMBER]: 'number',
  [PropertyKind.DATE]: 'date',
  [PropertyKind.SELECT]: 'select',
  [PropertyKind.RELATION]: 'relation',
  [PropertyKind.RELATIONS]: 'relations',
}

// e.g. "priority (select: none|low|medium|high, default none)", "tags (relations -> Tag)".
function describeProperty(p: PropertyDef): string {
  const kind = KIND_NAMES[p.kind] ?? 'unknown'
  if (p.kind === PropertyKind.SELECT) {
    const keys = p.options.map((o) => o.key).join('|')
    return `${p.id} (${kind}: ${keys}${p.defaultOption ? `, default ${p.defaultOption}` : ''})`
  }
  if (p.kind === PropertyKind.RELATION || p.kind === PropertyKind.RELATIONS) {
    return `${p.id} (${kind} -> ${p.targetStructure || 'any'})`
  }
  return `${p.id} (${kind})`
}

// The registry lives on the server (StructureService); this only formats it.
export async function listStructures(): Promise<string> {
  const { structures } = await structureClient.list({})
  return structures
    .map((s) => {
      const line = `- ${s.type}: ${s.description}`
      if (s.properties.length === 0) return line
      return `${line}\n  properties: ${s.properties.map(describeProperty).join(', ')}`
    })
    .join('\n')
}
