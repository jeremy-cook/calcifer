import { create } from '@bufbuild/protobuf'
import { timestampFromMs } from '@bufbuild/protobuf/wkt'
import { EntitySchema, RichTextSchema, type Entity, type RichText } from '@calcifer/proto/calcifer/v1/entities_pb'
import { WatchEntitiesResponseSchema, type WatchEntitiesResponse } from '@calcifer/proto/calcifer/v1/services_pb'

// Builders for the model tests. Not imported by the app.

export function entity(id: string, name = id, updatedAtMs = 1_000): Entity {
  return create(EntitySchema, { id, name, structureType: 'Note', updatedAt: timestampFromMs(updatedAtMs) })
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
