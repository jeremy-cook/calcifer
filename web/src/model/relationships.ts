import { useMemo } from 'react'
import type { Entity, LinkRef } from 'calcifer/v1/entities_pb'
import { timestampMsOrZero } from '~/model/dates'
import { RELATED_SCOPE, useAllEntities, useEntity } from '~/model/store'
import { getFacet, getStructure, propertyDefFor, richTextPropertyIds } from '~/model/structures'

// Why another entity is on the relationships section (ADR 12).
export type RelationshipReason =
  // a direct relationship
  | { kind: 'related' }
  // a rich-text link
  | { kind: 'mentioned' }
  // a relation property; back-references only
  | { kind: 'property'; propertyId: string; label: string }

export interface RelationshipRow {
  entityId: string
  structureType: string
  name: string
  // `outgoing`: a link from the entity to this one. `incoming`: a link from this one to the entity.
  direction: 'outgoing' | 'incoming'
  // Related first, then mentioned, then properties in registry order.
  reasons: RelationshipReason[]
  // The newest `createdAt` among the links the row was built from.
  mostRecentAt: Date
}

// A link that belongs on the section, with the reason it's there.
interface Reason {
  reason: RelationshipReason
  createdAtMs: number
}

// The reason `link`, written by `source`, puts its target on the section; null when it
// doesn't. An entity's own relation-property links (`tags`) show in their property
// fields, so only back-references get a `property` reason. Other `@` scopes are reserved
// for links the server derives and aren't listed.
function reasonOf(source: Entity, link: LinkRef, direction: RelationshipRow['direction']): RelationshipReason | null {
  const id = link.sourcePropertyId
  if (id === RELATED_SCOPE) return { kind: 'related' }
  if (id.startsWith('@')) return null
  if (richTextPropertyIds(source.structureType).includes(id)) return { kind: 'mentioned' }
  if (direction === 'outgoing') return null
  return { kind: 'property', propertyId: id, label: propertyDefFor(source, id)?.label || id }
}

// A property's position among its owner's declared properties: the structure's, then each
// attached facet's. Undeclared ids sort after them.
function propertyRank(owner: Entity, propertyId: string): number {
  const declared = [
    ...(getStructure(owner.structureType)?.properties ?? []),
    ...owner.facets.flatMap((f) => getFacet(f)?.properties ?? []),
  ]
  const index = declared.findIndex((p) => p.id === propertyId)
  return index === -1 ? declared.length : index
}

// Related, then mentioned, then properties in the owner's registry order.
function compareReasons(owner: Entity, a: RelationshipReason, b: RelationshipReason): number {
  const kindRank = { related: 0, mentioned: 1, property: 2 } as const
  if (a.kind !== b.kind) return kindRank[a.kind] - kindRank[b.kind]
  if (a.kind !== 'property' || b.kind !== 'property') return 0
  return (
    propertyRank(owner, a.propertyId) - propertyRank(owner, b.propertyId) || a.propertyId.localeCompare(b.propertyId)
  )
}

function reasonKey(reason: RelationshipReason): string {
  return reason.kind === 'property' ? `property:${reason.propertyId}` : reason.kind
}

// The row for `other` from the `links` that put it on the section. `owner` is the entity
// that wrote the links. Null when none of them do.
function buildRow(
  other: Entity,
  owner: Entity,
  direction: RelationshipRow['direction'],
  found: Reason[],
): RelationshipRow | null {
  if (found.length === 0) return null
  const reasons = new Map<string, RelationshipReason>()
  let mostRecentMs = 0
  for (const { reason, createdAtMs } of found) {
    reasons.set(reasonKey(reason), reason)
    mostRecentMs = Math.max(mostRecentMs, createdAtMs)
  }
  return {
    entityId: other.id,
    structureType: other.structureType,
    name: other.name,
    direction,
    reasons: [...reasons.values()].sort((a, b) => compareReasons(owner, a, b)),
    mostRecentAt: new Date(mostRecentMs),
  }
}

// Direct relationships first (outgoing, then incoming), then the rest; newest first within each.
function groupRank(row: RelationshipRow): number {
  if (!row.reasons.some((r) => r.kind === 'related')) return 2
  return row.direction === 'outgoing' ? 0 : 1
}

function compareRows(a: RelationshipRow, b: RelationshipRow): number {
  const byGroup = groupRank(a) - groupRank(b)
  if (byGroup !== 0) return byGroup
  const byTime = b.mostRecentAt.getTime() - a.mostRecentAt.getTime()
  if (byTime !== 0) return byTime
  // A stable order for ties, whatever order the replica holds the entities in.
  return a.entityId.localeCompare(b.entityId) || a.direction.localeCompare(b.direction)
}

// What the relationships section lists for `self`: one row per other entity and direction.
// The server derives every link it reads (ADR 3, ADR 12). Self-links are excluded, as
// `ListBacklinks` excludes them, and so are links to entities the replica doesn't hold.
export function selectRelationshipRows(entities: Entity[], self: Entity): RelationshipRow[] {
  const rows: RelationshipRow[] = []
  const byId = new Map(entities.map((e) => [e.id, e]))

  const outgoing = new Map<string, Reason[]>()
  for (const link of self.links) {
    const targetId = link.target?.id
    if (targetId === undefined || targetId === self.id) continue
    const reason = reasonOf(self, link, 'outgoing')
    if (!reason) continue
    const found = outgoing.get(targetId) ?? []
    found.push({ reason, createdAtMs: timestampMsOrZero(link.createdAt) })
    outgoing.set(targetId, found)
  }
  for (const [targetId, found] of outgoing) {
    const target = byId.get(targetId)
    const row = target && buildRow(target, self, 'outgoing', found)
    if (row) rows.push(row)
  }

  for (const source of entities) {
    if (source.id === self.id) continue
    const found: Reason[] = []
    for (const link of source.links) {
      if (link.target?.id !== self.id) continue
      const reason = reasonOf(source, link, 'incoming')
      if (reason) found.push({ reason, createdAtMs: timestampMsOrZero(link.createdAt) })
    }
    const row = buildRow(source, source, 'incoming', found)
    if (row) rows.push(row)
  }

  return rows.sort(compareRows)
}

// Empty while the entity isn't in the replica.
export function useRelationshipRows(entityId: string): RelationshipRow[] {
  const entities = useAllEntities()
  const { entity } = useEntity(entityId)
  return useMemo(() => (entity ? selectRelationshipRows(entities, entity) : []), [entities, entity])
}
