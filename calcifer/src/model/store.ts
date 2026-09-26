import { useCallback, useEffect, useRef } from 'react'
import type { ConnectError } from '@connectrpc/connect'
import { create as createMessage } from '@bufbuild/protobuf'
import { timestampNow } from '@bufbuild/protobuf/wkt'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  EntitySchema,
  PropertySchema,
  PropertyValueSchema,
  RichTextRefSchema,
  type Entity,
  type RichTextRef,
  type Property,
  type PropertyValue,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { PropertyKind, getStructure } from '~/model/structures'
import { entityClient, qk, queryClient } from '~/model/api'
import { fetchRichText, isRichTextEmpty, whenRichTextSaved } from '~/model/richtext'

export type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

function defaultNameFor(structureType: string): string {
  return `Untitled ${getStructure(structureType)?.name ?? structureType}`
}

// Entities are still constructed client-side (id minted here) so the editor has
// its richtext refs immediately; the server persists via Create.
function buildEntityMessage(structureType: string, name?: string): Entity {
  const id = crypto.randomUUID()
  const now = timestampNow()
  const properties: Property[] = []
  for (const def of getStructure(structureType)?.properties ?? []) {
    if (def.kind === PropertyKind.RICHTEXT) {
      const ref = createMessage(RichTextRefSchema, { entityId: id, propertyId: def.id })
      properties.push(
        createMessage(PropertySchema, {
          id: def.id,
          value: createMessage(PropertyValueSchema, { value: { case: 'richtext', value: ref } }),
        }),
      )
    } else if (def.kind === PropertyKind.SELECT && def.defaultOption !== '') {
      properties.push(
        createMessage(PropertySchema, {
          id: def.id,
          value: createMessage(PropertyValueSchema, { value: { case: 'select', value: def.defaultOption } }),
        }),
      )
    }
  }
  return createMessage(EntitySchema, {
    id,
    structureType,
    name: name ?? defaultNameFor(structureType),
    properties,
    links: [],
    referencedDates: [],
    createdAt: now,
    updatedAt: now,
  })
}

// --- Queries ---

export function useAllEntities(): Entity[] {
  const { data } = useQuery({
    queryKey: qk.entities(),
    queryFn: async () => (await entityClient.list({ structureType: '' })).entities,
  })
  return data ?? []
}

export function useEntity(id: string) {
  return useQuery({
    queryKey: qk.entity(id),
    queryFn: () => entityClient.get({ id }),
    enabled: !!id,
  })
}

// --- Mutations / actions ---

function onEntityWritten(entity: Entity) {
  queryClient.setQueryData(qk.entity(entity.id), entity)
  void queryClient.invalidateQueries({ queryKey: ['entities'] })
}

export function useCreateEntity() {
  const m = useMutation({
    mutationFn: (entity: Entity) => entityClient.create({ entity }),
    onSuccess: onEntityWritten,
  })
  return useCallback(
    (structureType: string, name?: string) =>
      m.mutateAsync(buildEntityMessage(structureType, name)),
    [m],
  )
}

// Get-or-create the day's DailyNote. The server builds it and names it for the day.
export function useResolveDailyNote() {
  const m = useMutation({
    mutationFn: async (iso: string) => {
      const { entity } = await entityClient.resolve({
        key: { case: 'date', value: iso },
        createIfMissing: true,
      })
      if (!entity) throw new Error(`resolve returned no daily note for ${iso}`)
      return entity
    },
    onSuccess: onEntityWritten,
  })
  return useCallback((iso: string) => m.mutateAsync(iso), [m])
}

export interface UseUpdateEntityOptions {
  // Called after the optimistic change has been rolled back.
  onError?: (err: ConnectError | Error, entity: Entity) => void
}

// Whole-entity write, for renames and multi-field edits. Optimistic on both the
// entity cache and the list cache, rolled back on error. Sends every property, so
// it can overwrite a concurrent writer's edit; single-property edits go through
// useSetProperty.
export function useUpdateEntity({ onError }: UseUpdateEntityOptions = {}) {
  const qc = useQueryClient()
  // Latest callback without re-creating the mutation on every render.
  const onErrorRef = useRef(onError)
  useEffect(() => {
    onErrorRef.current = onError
  })
  const { mutate } = useMutation({
    mutationFn: (entity: Entity) => entityClient.update({ entity }),
    onMutate: async (entity) => {
      await Promise.all([
        qc.cancelQueries({ queryKey: qk.entity(entity.id) }),
        qc.cancelQueries({ queryKey: qk.entities() }),
      ])
      const prevEntity = qc.getQueryData<Entity>(qk.entity(entity.id))
      const prevListEntity = qc.getQueryData<Entity[]>(qk.entities())?.find((e) => e.id === entity.id)
      qc.setQueryData(qk.entity(entity.id), entity)
      qc.setQueryData<Entity[]>(qk.entities(), (list) => list?.map((e) => (e.id === entity.id ? entity : e)))
      return { prevEntity, prevListEntity }
    },
    // Roll back only the failed entity's row: restoring a whole-list snapshot
    // would also undo other updates that were in flight concurrently.
    onError: (err, entity, ctx) => {
      const prev = ctx?.prevEntity ?? ctx?.prevListEntity
      if (prev) {
        qc.setQueryData(qk.entity(entity.id), prev)
        qc.setQueryData<Entity[]>(qk.entities(), (list) => list?.map((e) => (e.id === entity.id ? prev : e)))
      }
      onErrorRef.current?.(err, entity)
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ['entities'] })
    },
  })
  return useCallback((entity: Entity) => mutate(entity), [mutate])
}

export type UseSetPropertyOptions = UseUpdateEntityOptions

interface SetPropertyVars {
  entity: Entity
  propertyId: string
  value: PropertyValue['value'] | null
}

// A property's current value on `entity`: `null` when it has no such property.
function propertyValueOf(entity: Entity, propertyId: string): PropertyValue['value'] | null {
  const property = entity.properties.find((p) => p.id === propertyId)
  return property ? (property.value?.value ?? { case: undefined }) : null
}

// Write one property (`null` clears it) through EntityService.SetProperty, so
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
    mutationFn: ({ entity, propertyId, value }: SetPropertyVars) =>
      entityClient.setProperty({
        entityId: entity.id,
        propertyId,
        value: value === null ? undefined : createMessage(PropertyValueSchema, { value }),
      }),
    onMutate: async ({ entity, propertyId, value }) => {
      await Promise.all([
        qc.cancelQueries({ queryKey: qk.entity(entity.id) }),
        qc.cancelQueries({ queryKey: qk.entities() }),
      ])
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
    onSuccess: (saved) => {
      qc.setQueryData(qk.entity(saved.id), saved)
    },
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
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ['entities'] })
    },
  })
  return useCallback(
    (entity: Entity, propertyId: string, value: PropertyValue['value'] | null) => mutate({ entity, propertyId, value }),
    [mutate],
  )
}

// --- Pure edit builders (pass the result to useUpdateEntity(); withProperty is
// also useSetProperty()'s optimistic apply) ---

export function withName(entity: Entity, name: string): Entity {
  return createMessage(EntitySchema, { ...entity, name })
}

// `null` removes the property. The server still clears links for declared
// relation properties that are absent (SetProperty also for ad-hoc ones), so
// removing the last ref is safe.
export function withProperty(entity: Entity, propertyId: string, value: PropertyValue['value'] | null): Entity {
  if (value === null) {
    return createMessage(EntitySchema, { ...entity, properties: entity.properties.filter((p) => p.id !== propertyId) })
  }
  const next = createMessage(PropertySchema, { id: propertyId, value: createMessage(PropertyValueSchema, { value }) })
  const exists = entity.properties.some((p) => p.id === propertyId)
  const properties = exists ? entity.properties.map((p) => (p.id === propertyId ? next : p)) : [...entity.properties, next]
  return createMessage(EntitySchema, { ...entity, properties })
}

export function useDeleteEntity() {
  const m = useMutation({
    mutationFn: (id: string) => entityClient.delete({ id }),
    onSuccess: (_r, id) => {
      queryClient.removeQueries({ queryKey: qk.entity(id) })
      void queryClient.invalidateQueries({ queryKey: ['entities'] })
    },
  })
  return useCallback((id: string) => void m.mutateAsync(id), [m])
}

// --- Imperative (non-hook) helpers for the editor mention flow + cleanup ---

export function getEntitiesSnapshot(): Entity[] {
  return queryClient.getQueryData<Entity[]>(qk.entities()) ?? []
}

export async function getOrCreateEntityForMention(
  structureType: string,
  name: string,
): Promise<{ id: string; name: string; structureType: string }> {
  // Server-authoritative get-or-create so the browser and the MCP agent share
  // one identity path. The server dedupes by (structureType, name).
  const { entity } = await entityClient.resolve({
    structureType,
    key: { case: 'name', value: name },
    createIfMissing: true,
  })
  if (!entity) throw new Error(`resolve returned no entity for ${structureType} "${name}"`)
  onEntityWritten(entity)
  return { id: entity.id, name: entity.name, structureType: entity.structureType }
}

export function deleteEntityImperative(id: string): void {
  void entityClient
    .delete({ id })
    .then(() => {
      queryClient.removeQueries({ queryKey: qk.entity(id) })
      void queryClient.invalidateQueries({ queryKey: ['entities'] })
    })
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
        if (isRichTextEmpty(doc)) deleteEntityImperative(id)
      })
      .catch((err) => console.error('empty check before delete failed', err))
  }, 0)
}

// --- Pure derivations over an entity array (consumers pass useAllEntities()) ---

function updatedAtMillis(entity: Entity): number {
  const ts = entity.updatedAt
  if (!ts) return 0
  return Number(ts.seconds) * 1000 + ts.nanos / 1_000_000
}

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
  return matches.sort((a, b) => updatedAtMillis(b) - updatedAtMillis(a))
}

export function dailyNoteDate(entity: Entity): string | undefined {
  if (entity.structureType !== 'DailyNote') return undefined
  const value = entity.properties.find((p) => p.id === 'date')?.value?.value
  return value?.case === 'date' ? value.value : undefined
}

export function dailyNoteByDate(entities: Entity[], iso: string): Entity | undefined {
  return entities.find((e) => dailyNoteDate(e) === iso)
}

export function entityUpdatedAtDate(entity: Entity): Date {
  return new Date(updatedAtMillis(entity))
}
