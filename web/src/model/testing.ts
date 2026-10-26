import { create } from '@bufbuild/protobuf'
import { timestampFromMs } from '@bufbuild/protobuf/wkt'
import {
  EntitySchema,
  LinkRefSchema,
  RichTextSchema,
  type Entity,
  type LinkRef,
  type RichText,
} from 'calcifer/v1/entities_pb'
import { WatchEntitiesResponseSchema, type WatchEntitiesResponse } from 'calcifer/v1/services_pb'

// Builders for the model tests. Not imported by the app.

export function entity(id: string, name = id, updatedAtMs = 1_000): Entity {
  return create(EntitySchema, { id, name, structureType: 'Note', updatedAt: timestampFromMs(updatedAtMs) })
}

// `e` as another structure type.
export function ofType(e: Entity, structureType: string): Entity {
  return create(EntitySchema, { ...e, structureType })
}

// An outgoing link to `targetId`, derived from `sourcePropertyId` at `createdAtMs`.
export function link(targetId: string, sourcePropertyId: string, createdAtMs = 1_000, targetType = 'Note'): LinkRef {
  return create(LinkRefSchema, {
    id: `${sourcePropertyId}>${targetId}`,
    target: { id: targetId, structureType: targetType },
    sourcePropertyId,
    createdAt: timestampFromMs(createdAtMs),
  })
}

// `e` with these links, replacing any it had.
export function withLinks(e: Entity, ...links: LinkRef[]): Entity {
  return create(EntitySchema, { ...e, links })
}

export function richText(entityId: string, propertyId: string, doc: string, updatedAtMs: number): RichText {
  return create(RichTextSchema, { ref: { entityId, propertyId }, doc, updatedAt: timestampFromMs(updatedAtMs) })
}

export const watch = {
  snapshot: (...entities: Entity[]): WatchEntitiesResponse =>
    create(WatchEntitiesResponseSchema, { event: { case: 'snapshot', value: { entities } } }),
  upserted: (value: Entity): WatchEntitiesResponse =>
    create(WatchEntitiesResponseSchema, { event: { case: 'upserted', value } }),
  deletedId: (value: string): WatchEntitiesResponse =>
    create(WatchEntitiesResponseSchema, { event: { case: 'deletedId', value } }),
  richTextChanged: (value: RichText): WatchEntitiesResponse =>
    create(WatchEntitiesResponseSchema, { event: { case: 'richTextChanged', value } }),
}
