// Knowledge operations the MCP tools delegate to. Everything goes through the
// tonic core: agents author markdown, the server derives the link graph.
import { Code, ConnectError } from '@connectrpc/connect'
import { timestampFromMs, type Timestamp } from '@bufbuild/protobuf/wkt'
import { entityClient, richTextClient, searchClient, structureClient } from './calciferClient.js'
import { PropertyKind, type PropertyDef, type StructureDef } from '@calcifer/proto/calcifer/v1/structures_pb'
import { SearchMode as ProtoSearchMode, type MatchRange } from '@calcifer/proto/calcifer/v1/services_pb'
import { toTipTap } from './markdown/parse.js'
import { chipIds, fromTipTap } from './markdown/serialize.js'
import type { ChipTarget, Resolver, TTNode } from './markdown/types.js'

// Get (or, with createIfMissing, create) the (structureType, name) entity.
// NOT_FOUND when it's missing and createIfMissing is false.
function resolveName(structureType: string, name: string, createIfMissing: boolean) {
  return entityClient.resolveEntity({ structureType, key: { case: 'name', value: name }, createIfMissing })
}

// The structure registry, fetched once per process (it's static for a server's
// lifetime). A failed fetch isn't cached, so the next call retries.
let registry: Promise<StructureDef[]> | undefined

function structures(): Promise<StructureDef[]> {
  registry ??= structureClient.listStructures({}).then(
    (r) => r.structures,
    (e: unknown) => {
      registry = undefined
      throw e
    },
  )
  return registry
}

// A structure's declared rich-text property ids, in registry order.
function richTextIds(all: StructureDef[], structureType: string): string[] {
  const def = all.find((s) => s.type === structureType)
  return (def?.properties ?? []).filter((p) => p.kind === PropertyKind.RICHTEXT).map((p) => p.id)
}

async function structureTypes(): Promise<Set<string>> {
  return new Set((await structures()).map((s) => s.type))
}

// [[wikilinks]] / #tags resolve to canonical entities server-side. A missing
// target is created only for a creatable structure; otherwise (a DailyNote)
// it's null and the chip becomes plain text.
const resolver: Resolver = async (structureType, name) => {
  const creatable = (await structures()).find((s) => s.type === structureType)?.creatable ?? false
  try {
    const r = await resolveName(structureType, name, creatable)
    return { id: r.entity!.id, name: r.entity!.name }
  } catch (e) {
    if (!creatable && isNotFound(e)) return null
    throw e
  }
}

async function parseMarkdown(markdown: string): Promise<TTNode> {
  return toTipTap(markdown, resolver, await structureTypes())
}

// Each chip target's current name and structure, fetched in parallel. A target
// that no longer exists is left out.
async function chipTargets(doc: TTNode): Promise<Map<string, ChipTarget>> {
  const found = await Promise.all(
    chipIds(doc).map(async (id) => {
      try {
        const e = (await entityClient.getEntity({ id })).entity!
        return [id, { name: e.name, structureType: e.structureType }] as const
      } catch (err) {
        if (isNotFound(err)) return null
        throw err
      }
    }),
  )
  return new Map(found.filter((f) => f !== null))
}

async function toMarkdown(doc: TTNode): Promise<string> {
  const [targets, types] = await Promise.all([chipTargets(doc), structureTypes()])
  return fromTipTap(doc, targets, types)
}

interface DocRef {
  entityId: string
  propertyId: string
}

// The body doc of an entity: its structure's first declared rich-text property.
export async function docRef(structureType: string, entityId: string): Promise<DocRef> {
  const [propertyId] = richTextIds(await structures(), structureType)
  if (!propertyId) throw new Error(`Structure "${structureType}" declares no rich-text property, so it has no body to read or write.`)
  return { entityId, propertyId }
}

const emptyDoc = (): TTNode => ({ type: 'doc', content: [] })

function isNotFound(e: unknown): boolean {
  return e instanceof ConnectError && e.code === Code.NotFound
}

// A declared-but-unsaved doc comes back empty at the epoch (see RichTextService.GetRichText),
// so `updatedAt` is always something a conditional PutRichText can echo back.
async function getDoc(ref: DocRef): Promise<{ doc: TTNode; updatedAt: Timestamp }> {
  const rt = (await richTextClient.getRichText(ref)).richText!
  return {
    doc: rt.doc ? (JSON.parse(rt.doc) as TTNode) : emptyDoc(),
    updatedAt: rt.updatedAt ?? timestampFromMs(0),
  }
}

const APPEND_ATTEMPTS = 3

// Read, merge, and PutRichText conditioned on the updated_at we read. A concurrent
// save (e.g. the browser) makes the PutRichText fail with FAILED_PRECONDITION; re-read and retry.
async function appendDoc(ref: DocRef, label: string, markdown: string): Promise<void> {
  const addition = await parseMarkdown(markdown)
  for (let attempt = 1; attempt <= APPEND_ATTEMPTS; attempt++) {
    const { doc, updatedAt } = await getDoc(ref)
    const merged: TTNode = { type: 'doc', content: [...(doc.content ?? []), ...(addition.content ?? [])] }
    try {
      await richTextClient.putRichText({ ...ref, doc: JSON.stringify(merged), expectedUpdatedAt: updatedAt })
      return
    } catch (e) {
      if (!(e instanceof ConnectError && e.code === Code.FailedPrecondition)) throw e
    }
  }
  throw new Error(`${label} kept changing while appending (${APPEND_ATTEMPTS} attempts); nothing was appended. Try again.`)
}

interface BacklinkSource {
  name: string
  structureType: string
  propertyIds: string[]
}

// The entities linking to `targetId`, newest link first. The server returns one
// Backlink per link row (self-links excluded), so a source that links from two
// properties is folded into one entry here.
async function backlinkSources(targetId: string): Promise<BacklinkSource[]> {
  const res = await entityClient.listBacklinks({ entityId: targetId })
  const byId = new Map<string, BacklinkSource>()
  for (const b of res.backlinks) {
    const source = b.source!
    const entry = byId.get(source.id) ?? { name: source.name, structureType: source.structureType, propertyIds: [] }
    entry.propertyIds.push(b.sourcePropertyId)
    byId.set(source.id, entry)
  }
  return [...byId.values()]
}

// The source's name, plus the linking properties unless they're all rich-text docs.
function backlinkLabel({ name, structureType, propertyIds }: BacklinkSource, all: StructureDef[]): string {
  const docs = richTextIds(all, structureType)
  return propertyIds.every((p) => docs.includes(p)) ? name : `${name} (via ${propertyIds.join(', ')})`
}

export async function createNote(name: string, markdown: string): Promise<string> {
  const id = (await resolveName('Note', name, true)).entity!.id
  const doc = await parseMarkdown(markdown)
  // Unconditional on purpose: create_note overwrites whatever the note held.
  await richTextClient.putRichText({ ...(await docRef('Note', id)), doc: JSON.stringify(doc) })
  const ent = (await entityClient.getEntity({ id })).entity!
  const linked = ent.links.map((l) => l.target?.structureType).join(', ') || 'none'
  return `Saved note "${ent.name}" (${id}). Derived links: ${ent.links.length} [${linked}]; dates: ${ent.referencedDates.length}.`
}

export async function appendToNote(name: string, markdown: string): Promise<string> {
  const id = (await resolveName('Note', name, true)).entity!.id
  await appendDoc(await docRef('Note', id), `Note "${name}"`, markdown)
  const ent = (await entityClient.getEntity({ id })).entity!
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
  const md = (await toMarkdown((await getDoc(await docRef('Note', entityId))).doc)).trim()
  const back = (await backlinkSources(entityId)).map((s) => s.name)
  return `# ${entityName}\n\n${md || '(empty)'}\n\n---\nLinked from: ${back.join(', ') || '(nothing)'}`
}

export type SearchMode = 'lexical' | 'semantic' | 'hybrid'

const searchModes: Record<SearchMode, ProtoSearchMode> = {
  lexical: ProtoSearchMode.LEXICAL,
  semantic: ProtoSearchMode.SEMANTIC,
  hybrid: ProtoSearchMode.HYBRID,
}

// Bold each matched range of a plain-text snippet. `**` rather than brackets, so
// a match never reads as [[wikilink]] syntax. Ranges are UTF-16 offsets, which is
// what JS string indices count.
function markMatches(snippet: string, matches: MatchRange[]): string {
  let out = ''
  let at = 0
  for (const m of matches) {
    out += `${snippet.slice(at, m.start)}**${snippet.slice(m.start, m.end)}**`
    at = m.end
  }
  return out + snippet.slice(at)
}

export async function searchNotes(
  query: string,
  limit = 10,
  mode: SearchMode = 'hybrid',
): Promise<string> {
  // The server falls back to lexical for semantic/hybrid when embeddings are off.
  const res = await searchClient.search({ query, limit, mode: searchModes[mode] })
  if (res.hits.length === 0) return `No matches for "${query}".`
  return res.hits
    .map((h) => {
      const detail = h.snippet ? markMatches(h.snippet, h.matches) : `(score ${h.score.toFixed(3)})`
      return `- [${h.entity!.structureType}] ${h.entity!.name}: ${detail}`
    })
    .join('\n')
}

export async function getBacklinks(name: string): Promise<string> {
  const r = await resolveName('Note', name, false).catch(() => null)
  if (!r?.entity) return `No note named "${name}".`
  const [sources, all] = await Promise.all([backlinkSources(r.entity.id), structures()])
  return sources.length
    ? sources.map((s) => `- ${backlinkLabel(s, all)}`).join('\n')
    : `Nothing links to "${name}" yet.`
}

export async function linkNotes(from: string, to: string): Promise<string> {
  // Links are content-derived, so "linking" = adding a [[to]] mention to `from`.
  await appendToNote(from, `Related: [[${to}]]`)
  return `Linked "${from}" -> "${to}".`
}

// Get-or-create the DailyNote for `date` (ISO "YYYY-MM-DD"). The server keeps
// one per day and names it for the day.
async function resolveDailyNote(date: string): Promise<{ id: string; name: string; created: boolean }> {
  const r = await entityClient.resolveEntity({ key: { case: 'date', value: date }, createIfMissing: true })
  return { id: r.entity!.id, name: r.entity!.name, created: r.created }
}

export async function createDailyNote(date: string): Promise<string> {
  const { id, name, created } = await resolveDailyNote(date)
  return `Daily note "${name}" (${id}) for ${date} ${created ? 'created' : 'already existed'}.`
}

export async function appendToDailyNote(date: string, markdown: string): Promise<string> {
  const { id, name } = await resolveDailyNote(date)
  await appendDoc(await docRef('DailyNote', id), `Daily note "${name}"`, markdown)
  const ent = (await entityClient.getEntity({ id })).entity!
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
  return (await structures())
    .map((s) => {
      const line = `- ${s.type}: ${s.description}`
      if (s.properties.length === 0) return line
      return `${line}\n  properties: ${s.properties.map(describeProperty).join(', ')}`
    })
    .join('\n')
}
