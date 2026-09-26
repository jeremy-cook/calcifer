import { useCallback, useEffect, useMemo, useRef } from 'react'
import type { ConnectError } from '@connectrpc/connect'
import { create as createMessage } from '@bufbuild/protobuf'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  EntityRefListSchema,
  EntitySchema,
  PropertyValueSchema,
  type Entity,
  type RichTextRef,
  type PropertyValue,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import type { ResolveEntityRequest } from '@calcifer/proto/calcifer/v1/services_pb'
import { entityClient, qk, queryClient } from '~/model/api'
import { timestampMsOrZero } from '~/model/dates'
import { fetchRichText, isRichTextEmpty, whenRichTextSaved } from '~/model/richtext'
import { entitiesQuery, removeEntity, writeEntity } from '~/model/sync'

export type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

// The entity RPCs wrap their entity in a response message; the server always
// sets it, so a missing one is a protocol error.
function requireEntity({ entity }: { entity?: Entity }): Entity {
  if (!entity) throw new Error('response carried no entity')
  return entity
}

// --- Queries ---

// The Watch replica (`model/sync.ts`): empty until the first snapshot arrives.
export function useAllEntities(): Entity[] {
  return useQuery(entitiesQuery).data ?? []
}

// Seeded from the replica. `GetEntity` runs only for an entity the replica doesn't
// hold yet (a deep link before the first snapshot) or doesn't hold at all;
// after that Watch and the mutations keep it current, so it never refetches.
export function useEntity(id: string) {
  return useQuery({
    queryKey: qk.entity(id),
    queryFn: async () => requireEntity(await entityClient.getEntity({ id })),
    initialData: () => getEntitiesSnapshot().find((e) => e.id === id),
    staleTime: Infinity,
    enabled: !!id,
  })
}

// --- Mutations / actions ---

interface CreateEntityVars {
  structureType: string
  name?: string
}

// Create by intent: the server mints the id and builds the defaults, including
// the default name when `name` is omitted. Resolves to the saved entity.
export function useCreateEntity() {
  const m = useMutation({
    mutationFn: async ({ structureType, name }: CreateEntityVars) =>
      requireEntity(await entityClient.createEntity({ structureType, name })),
    onSuccess: writeEntity,
  })
  return useCallback(
    (structureType: string, name?: string) => m.mutateAsync({ structureType, name }),
    [m],
  )
}

// Server-authoritative get-or-create through EntityService.ResolveEntity, so the
// browser and the MCP agent share one identity path. Writes the result into the caches.
async function resolveOrCreateEntity(structureType: string, key: ResolveEntityRequest['key']): Promise<Entity> {
  const entity = requireEntity(await entityClient.resolveEntity({ structureType, key, createIfMissing: true }))
  writeEntity(entity)
  return entity
}

// Get-or-create the day's DailyNote. The server builds it and names it for the day.
export function useResolveDailyNote() {
  const m = useMutation({
    mutationFn: (iso: string) => resolveOrCreateEntity('', { case: 'date', value: iso }),
  })
  return useCallback((iso: string) => m.mutateAsync(iso), [m])
}

interface RenameEntityVars {
  id: string
  name: string
}

function renamed(entity: Entity, name: string): Entity {
  return createMessage(EntitySchema, { ...entity, name })
}

// Rename through EntityService.RenameEntity, which writes only the name, so it can't
// undo a concurrent property edit. Optimistic on `name` in both caches. On
// error only the name is restored, on the current cached entity (the I-6
// per-entity rollback), so a concurrent edit to anything else survives.
export function useRenameEntity() {
  const qc = useQueryClient()
  const { mutate } = useMutation({
    mutationFn: async ({ id, name }: RenameEntityVars) => requireEntity(await entityClient.renameEntity({ id, name })),
    onMutate: async ({ id, name }) => {
      // The list is never refetched (Watch feeds it), so only a deep link's GetEntity can be in flight.
      await qc.cancelQueries({ queryKey: qk.entity(id) })
      const prevEntityName = qc.getQueryData<Entity>(qk.entity(id))?.name
      const prevListName = qc.getQueryData<Entity[]>(qk.entities())?.find((e) => e.id === id)?.name
      qc.setQueryData<Entity>(qk.entity(id), (e) => e && renamed(e, name))
      qc.setQueryData<Entity[]>(qk.entities(), (list) => list?.map((e) => (e.id === id ? renamed(e, name) : e)))
      return { prevEntityName, prevListName }
    },
    onSuccess: writeEntity,
    onError: (_err, { id }, ctx) => {
      if (ctx?.prevEntityName !== undefined) {
        const prev = ctx.prevEntityName
        qc.setQueryData<Entity>(qk.entity(id), (e) => e && renamed(e, prev))
      }
      if (ctx?.prevListName !== undefined) {
        const prev = ctx.prevListName
        qc.setQueryData<Entity[]>(qk.entities(), (list) => list?.map((e) => (e.id === id ? renamed(e, prev) : e)))
      }
    },
  })
  return useCallback((id: string, name: string) => mutate({ id, name }), [mutate])
}

export interface UseSetPropertyOptions {
  // Called after the optimistic change has been rolled back.
  onError?: (err: ConnectError | Error, entity: Entity) => void
}

interface SetPropertyVars {
  entity: Entity
  propertyId: string
  value: PropertyValue['value'] | null
}

// A property's current value on `entity`: `null` when it has no such property.
// `entity.properties` is a plain-object map, so an ad-hoc id such as
// `constructor` would otherwise read an `Object.prototype` member.
export function propertyValueOf(entity: Entity, propertyId: string): PropertyValue['value'] | null {
  return Object.hasOwn(entity.properties, propertyId) ? entity.properties[propertyId].value : null
}

// Write one property (`null` clears it) through EntityService.SetEntityProperty, so
// edits to different properties of the same entity don't overwrite each other.
// Optimistic on both caches, applied to the entity as currently cached (not the
// caller's possibly stale copy) so quick successive edits all show. On error
// only this property is restored, on the current cached entity: restoring a
// whole-entity snapshot would undo a concurrent edit to another property. On
// success the server's entity replaces the cached one, carrying anything the
// server derived from the write (e.g. a moved daily note's new name).
export function useSetProperty({ onError }: UseSetPropertyOptions = {}) {
  const qc = useQueryClient()
  // Latest callback without re-creating the mutation on every render.
  const onErrorRef = useRef(onError)
  useEffect(() => {
    onErrorRef.current = onError
  })
  const { mutate } = useMutation({
    mutationFn: async ({ entity, propertyId, value }: SetPropertyVars) =>
      requireEntity(
        await entityClient.setEntityProperty({
          entityId: entity.id,
          propertyId,
          value: value === null ? undefined : createMessage(PropertyValueSchema, { value }),
        }),
      ),
    onMutate: async ({ entity, propertyId, value }) => {
      // The list is never refetched (Watch feeds it), so only a deep link's GetEntity can be in flight.
      await qc.cancelQueries({ queryKey: qk.entity(entity.id) })
      const cachedEntity = qc.getQueryData<Entity>(qk.entity(entity.id))
      const cachedListEntity = qc.getQueryData<Entity[]>(qk.entities())?.find((e) => e.id === entity.id)
      const prevEntityValue = cachedEntity && propertyValueOf(cachedEntity, propertyId)
      const prevListValue = cachedListEntity && propertyValueOf(cachedListEntity, propertyId)
      qc.setQueryData<Entity>(qk.entity(entity.id), (e) => e && withProperty(e, propertyId, value))
      qc.setQueryData<Entity[]>(qk.entities(), (list) =>
        list?.map((e) => (e.id === entity.id ? withProperty(e, propertyId, value) : e)),
      )
      return { prevEntityValue, prevListValue }
    },
    onSuccess: writeEntity,
    onError: (err, { entity, propertyId }, ctx) => {
      if (ctx?.prevEntityValue !== undefined) {
        const prev = ctx.prevEntityValue
        qc.setQueryData<Entity>(qk.entity(entity.id), (e) => e && withProperty(e, propertyId, prev))
      }
      if (ctx?.prevListValue !== undefined) {
        const prev = ctx.prevListValue
        qc.setQueryData<Entity[]>(qk.entities(), (list) =>
          list?.map((e) => (e.id === entity.id ? withProperty(e, propertyId, prev) : e)),
        )
      }
      onErrorRef.current?.(err, entity)
    },
  })
  return useCallback(
    (entity: Entity, propertyId: string, value: PropertyValue['value'] | null) => mutate({ entity, propertyId, value }),
    [mutate],
  )
}

// A relation target: an `EntityRef`, or a plain `{ id, structureType }`.
export interface RelationTarget {
  id: string
  structureType: string
}

export interface PropertyWriters {
  // `null` clears the date.
  setDate: (entity: Entity, propertyId: string, iso: string | null) => void
  setSelect: (entity: Entity, propertyId: string, key: string) => void
  // An empty list clears the property.
  setRelations: (entity: Entity, propertyId: string, targets: readonly RelationTarget[]) => void
}

// Typed writes on top of useSetProperty, so callers never build a PropertyValue.
export function usePropertyWriters(options: UseSetPropertyOptions = {}): PropertyWriters {
  const setProperty = useSetProperty(options)
  return useMemo(
    () => ({
      setDate: (entity, propertyId, iso) =>
        setProperty(entity, propertyId, iso === null ? null : { case: 'date', value: iso }),
      setSelect: (entity, propertyId, key) => setProperty(entity, propertyId, { case: 'select', value: key }),
      setRelations: (entity, propertyId, targets) =>
        setProperty(
          entity,
          propertyId,
          targets.length === 0
            ? null
            : {
                case: 'relations',
                value: createMessage(EntityRefListSchema, {
                  refs: targets.map(({ id, structureType }) => ({ id, structureType })),
                }),
              },
        ),
    }),
    [setProperty],
  )
}

// --- Pure edit builder (useSetProperty()'s optimistic apply) ---

// `null` removes the property. The server still clears links for declared
// relation properties that are absent (SetEntityProperty also for ad-hoc ones), so
// removing the last ref is safe.
function withProperty(entity: Entity, propertyId: string, value: PropertyValue['value'] | null): Entity {
  const properties = { ...entity.properties }
  if (value === null) delete properties[propertyId]
  else properties[propertyId] = createMessage(PropertyValueSchema, { value })
  return createMessage(EntitySchema, { ...entity, properties })
}

// --- Imperative (non-hook) helpers: delete, the editor mention flow, cleanup ---

export function getEntitiesSnapshot(): Entity[] {
  return queryClient.getQueryData(entitiesQuery.queryKey) ?? []
}

export async function getOrCreateEntityForMention(
  structureType: string,
  name: string,
): Promise<{ id: string; name: string; structureType: string }> {
  // The server dedupes by (structureType, name).
  const entity = await resolveOrCreateEntity(structureType, { case: 'name', value: name })
  return { id: entity.id, name: entity.name, structureType: entity.structureType }
}

// The one delete path: fire-and-forget, dropped from the caches once the server confirms.
export function deleteEntity(id: string): void {
  void entityClient
    .deleteEntity({ id })
    .then(() => removeEntity(id))
    .catch((err) => console.error('delete failed', err))
}

// Deletes the entity only if the server's copy of `ref` is empty; reads the
// server, not the cache, so a stale cache can't discard an outside write.
// Local saves of `ref` (queued or in flight) land before the check.
export function deleteEntityIfRichTextEmpty(id: string, ref: RichTextRef): void {
  // Wait a macrotask first: callers run this from an effect cleanup, and React
  // may run that before or after the editor's unmount cleanup that flushes its
  // debounced save. By the next macrotask every cleanup in the commit has run,
  // so a flushed save is already counted as outstanding.
  setTimeout(() => {
    void whenRichTextSaved(ref)
      .then(() => fetchRichText(ref))
      .then(({ doc }) => {
        // Small race left: a write landing between this Get and the Delete is lost.
        if (isRichTextEmpty(doc)) deleteEntity(id)
      })
      .catch((err) => console.error('empty check before delete failed', err))
  }, 0)
}

// --- Pure derivations over an entity array (consumers pass useAllEntities()) ---

export function listByStructure(entities: Entity[], structureType: string): Entity[] {
  const matches = entities.filter((e) => e.structureType === structureType)
  if (structureType === 'DailyNote') {
    // ISO yyyy-MM-dd compares chronologically as a string. A missing date maps
    // to '', which is smallest, so undated notes sort last. Ties break by id.
    return matches.sort((a, b) => {
      const da = dailyNoteDate(a) ?? ''
      const db = dailyNoteDate(b) ?? ''
      if (da !== db) return da < db ? 1 : -1
      return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
    })
  }
  return matches.sort((a, b) => timestampMsOrZero(b.updatedAt) - timestampMsOrZero(a.updatedAt))
}

export function dailyNoteDate(entity: Entity): string | undefined {
  if (entity.structureType !== 'DailyNote') return undefined
  const value = entity.properties.date?.value
  return value?.case === 'date' ? value.value : undefined
}

export function dailyNoteByDate(entities: Entity[], iso: string): Entity | undefined {
  return entities.find((e) => dailyNoteDate(e) === iso)
}

export function entityUpdatedAtDate(entity: Entity): Date {
  return new Date(timestampMsOrZero(entity.updatedAt))
}
