// Knowledge operations the MCP tools delegate to. Everything goes through the
// tonic core: agents author markdown, the server derives the link graph.
import { Code, ConnectError } from '@connectrpc/connect'
import { timestampFromMs, type Timestamp } from '@bufbuild/protobuf/wkt'
import { entityClient, richTextClient, searchClient, structureClient } from './calciferClient.js'
import { PropertyKind, type PropertyDef } from '@calcifer/proto/calcifer/v1/structures_pb'
import { toTipTap } from './markdown/parse.js'
import { fromTipTap } from './markdown/serialize.js'
import type { TTNode } from './markdown/types.js'

// Get (or, with createIfMissing, create) the (structureType, name) entity.
// NOT_FOUND when it's missing and createIfMissing is false.
function resolveName(structureType: string, name: string, createIfMissing: boolean) {
  return entityClient.resolve({ structureType, key: { case: 'name', value: name }, createIfMissing })
}

// [[wikilinks]] / #tags resolve to canonical entities (get-or-create) server-side.
const resolver = async (structureType: string, name: string) => {
  const r = await resolveName(structureType, name, true)
  return { id: r.entity!.id, name: r.entity!.name }
}

const contentRef = (entityId: string) => ({ entityId, propertyId: 'content' })
const emptyDoc = (): TTNode => ({ type: 'doc', content: [] })

function isNotFound(e: unknown): boolean {
  return e instanceof ConnectError && e.code === Code.NotFound
}

// A declared-but-unsaved doc comes back empty at the epoch (see RichTextService.Get),
// so `updatedAt` is always something a conditional Put can echo back.
async function getDoc(entityId: string): Promise<{ doc: TTNode; updatedAt: Timestamp }> {
  const rt = await richTextClient.get(contentRef(entityId))
  return {
    doc: rt.doc ? (JSON.parse(rt.doc) as TTNode) : emptyDoc(),
    updatedAt: rt.updatedAt ?? timestampFromMs(0),
  }
}

const APPEND_ATTEMPTS = 3

// Read, merge, and Put conditioned on the updated_at we read. A concurrent save
// (e.g. the browser) makes the Put fail with FAILED_PRECONDITION; re-read and retry.
async function appendDoc(entityId: string, label: string, markdown: string): Promise<void> {
  const addition = await toTipTap(markdown, resolver)
  for (let attempt = 1; attempt <= APPEND_ATTEMPTS; attempt++) {
    const { doc, updatedAt } = await getDoc(entityId)
    const merged: TTNode = { type: 'doc', content: [...(doc.content ?? []), ...(addition.content ?? [])] }
    try {
      await richTextClient.put({ ref: contentRef(entityId), doc: JSON.stringify(merged), expectedUpdatedAt: updatedAt })
      return
    } catch (e) {
      if (!(e instanceof ConnectError && e.code === Code.FailedPrecondition)) throw e
    }
  }
  throw new Error(`${label} kept changing while appending (${APPEND_ATTEMPTS} attempts); nothing was appended. Try again.`)
}

async function backlinkNames(targetId: string): Promise<string[]> {
  // Server-side backlink scan (SELECT ... WHERE target_id = ?), no client-side List.
  const res = await entityClient.listBacklinks({ id: targetId, structureType: '' })
  return res.entities.map((e) => e.name)
}

export async function createNote(name: string, markdown: string): Promise<string> {
  const { id } = await resolver('Note', name)
  const doc = await toTipTap(markdown, resolver)
  // Unconditional on purpose: create_note overwrites whatever the note held.
  await richTextClient.put({ ref: contentRef(id), doc: JSON.stringify(doc) })
  const ent = await entityClient.get({ id })
  const linked = ent.links.map((l) => l.target?.structureType).join(', ') || 'none'
  return `Saved note "${ent.name}" (${id}). Derived links: ${ent.links.length} [${linked}]; dates: ${ent.referencedDates.length}.`
}

export async function appendToNote(name: string, markdown: string): Promise<string> {
  const { id } = await resolver('Note', name)
  await appendDoc(id, `Note "${name}"`, markdown)
  const ent = await entityClient.get({ id })
  return `Appended to "${ent.name}" (${id}); now ${ent.links.length} link(s).`
}

export async function getNote(name: string): Promise<string> {
  let entityId: string
  let entityName: string
  try {
    const r = await resolveName('Note', name, false)
    entityId = r.entity!.id
    entityName = r.entity!.name
  } catch (e) {
    if (isNotFound(e)) return `No note named "${name}". Use search_notes or create_note.`
    throw e
  }
  const md = fromTipTap((await getDoc(entityId)).doc).trim()
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
  const r = await resolveName('Note', name, false).catch(() => null)
  if (!r?.entity) return `No note named "${name}".`
  const names = await backlinkNames(r.entity.id)
  return names.length ? names.map((n) => `- ${n}`).join('\n') : `Nothing links to "${name}" yet.`
}

export async function linkNotes(from: string, to: string): Promise<string> {
  // Links are content-derived, so "linking" = adding a [[to]] mention to `from`.
  await appendToNote(from, `Related: [[${to}]]`)
  return `Linked "${from}" -> "${to}".`
}

// Get-or-create the DailyNote for `date` (ISO "YYYY-MM-DD"). The server keeps
// one per day and names it for the day.
async function resolveDailyNote(date: string): Promise<{ id: string; name: string; created: boolean }> {
  const r = await entityClient.resolve({ key: { case: 'date', value: date }, createIfMissing: true })
  return { id: r.entity!.id, name: r.entity!.name, created: r.created }
}

export async function createDailyNote(date: string): Promise<string> {
  const { id, name, created } = await resolveDailyNote(date)
  return `Daily note "${name}" (${id}) for ${date} ${created ? 'created' : 'already existed'}.`
}

export async function appendToDailyNote(date: string, markdown: string): Promise<string> {
  const { id, name } = await resolveDailyNote(date)
  await appendDoc(id, `Daily note "${name}"`, markdown)
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
