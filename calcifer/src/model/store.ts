import { useCallback } from 'react'
import { create as createMessage } from '@bufbuild/protobuf'
import { timestampNow } from '@bufbuild/protobuf/wkt'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  EntitySchema,
  PropertySchema,
  PropertyValueSchema,
  RichTextRefSchema,
  type Entity,
  type Property,
  type PropertyValue,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { STRUCTURES, type StructureType } from '~/model/structures'
import { formatLongDate } from '~/model/dates'
import { entityClient, qk, queryClient } from '~/model/api'

export type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

export type CreatableStructureType = Exclude<StructureType, 'DailyNote'>

function defaultNameFor(structureType: CreatableStructureType): string {
  return `Untitled ${STRUCTURES[structureType].name}`
}

// Entities are still constructed client-side (id minted here) so the editor has
// its richtext refs immediately; the server persists via Create.
function buildEntityMessage(structureType: CreatableStructureType, name?: string): Entity {
  const id = crypto.randomUUID()
  const now = timestampNow()
  const structure = STRUCTURES[structureType]
  const properties: Property[] = []
  for (const def of structure.properties) {
    if (def.type === 'richtext') {
      const ref = createMessage(RichTextRefSchema, { entityId: id, propertyId: def.id })
      properties.push(
        createMessage(PropertySchema, {
          id: def.id,
          value: createMessage(PropertyValueSchema, { value: { case: 'richtext', value: ref } }),
        }),
      )
    } else if (def.type === 'select' && def.default !== undefined) {
      properties.push(
        createMessage(PropertySchema, {
          id: def.id,
          value: createMessage(PropertyValueSchema, { value: { case: 'select', value: def.default } }),
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

function buildDailyNoteMessage(iso: string): Entity {
  const id = crypto.randomUUID()
  const now = timestampNow()
  const ref = createMessage(RichTextRefSchema, { entityId: id, propertyId: 'content' })
  const properties: Property[] = [
    createMessage(PropertySchema, {
      id: 'date',
      value: createMessage(PropertyValueSchema, { value: { case: 'date', value: iso } }),
    }),
    createMessage(PropertySchema, {
      id: 'content',
      value: createMessage(PropertyValueSchema, { value: { case: 'richtext', value: ref } }),
    }),
  ]
  return createMessage(EntitySchema, {
    id,
    structureType: 'DailyNote',
    name: formatLongDate(iso),
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
    (structureType: CreatableStructureType, name?: string) =>
      m.mutateAsync(buildEntityMessage(structureType, name)),
    [m],
  )
}

export function useCreateDailyNote() {
  const m = useMutation({
    mutationFn: (entity: Entity) => entityClient.create({ entity }),
    onSuccess: onEntityWritten,
  })
  return useCallback((iso: string) => m.mutateAsync(buildDailyNoteMessage(iso)), [m])
}

// Single write path for edits to an existing entity. Optimistic on both the
// entity cache and the list cache (so checkboxes on /s/Todo and the calendar
// flip immediately), rolled back on error. Sends the whole entity, so concurrent
// writers are last-write-wins.
export function useUpdateEntity() {
  const qc = useQueryClient()
  const { mutate } = useMutation({
    mutationFn: (entity: Entity) => entityClient.update({ entity }),
    onMutate: async (entity) => {
      await Promise.all([
        qc.cancelQueries({ queryKey: qk.entity(entity.id) }),
        qc.cancelQueries({ queryKey: qk.entities() }),
      ])
      const prevEntity = qc.getQueryData<Entity>(qk.entity(entity.id))
      const prevList = qc.getQueryData<Entity[]>(qk.entities())
      qc.setQueryData(qk.entity(entity.id), entity)
      qc.setQueryData<Entity[]>(qk.entities(), (list) => list?.map((e) => (e.id === entity.id ? entity : e)))
      return { prevEntity, prevList }
    },
    onError: (_e, entity, ctx) => {
      if (ctx?.prevEntity) qc.setQueryData(qk.entity(entity.id), ctx.prevEntity)
      if (ctx?.prevList) qc.setQueryData(qk.entities(), ctx.prevList)
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ['entities'] })
    },
  })
  return useCallback((entity: Entity) => mutate(entity), [mutate])
}

// --- Pure edit builders (pass the result to useUpdateEntity()) ---

export function withName(entity: Entity, name: string): Entity {
  return createMessage(EntitySchema, { ...entity, name })
}

// `null` removes the property. The server still clears links for declared
// relation properties that are absent, so removing the last ref is safe.
export function withProperty(entity: Entity, propertyId: string, value: PropertyValue['value'] | null): Entity {
  if (value === null) {
    return createMessage(EntitySchema, { ...entity, properties: entity.properties.filter((p) => p.id !== propertyId) })
  }
  const next = createMessage(PropertySchema, { id: propertyId, value: createMessage(PropertyValueSchema, { value }) })
  const exists = entity.properties.some((p) => p.id === propertyId)
  const properties = exists ? entity.properties.map((p) => (p.id === propertyId ? next : p)) : [...entity.properties, next]
  return createMessage(EntitySchema, { ...entity, properties })
}

// A daily note's name is derived from its date, so moving it renames it too.
export function withDailyNoteDate(entity: Entity, iso: string): Entity {
  return withName(withProperty(entity, 'date', { case: 'date', value: iso }), formatLongDate(iso))
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
  structureType: CreatableStructureType,
  name: string,
): Promise<{ id: string; name: string; structureType: string }> {
  // Server-authoritative get-or-create so the browser and the MCP agent share
  // one identity path. The server dedupes by (structureType, name).
  const { entity } = await entityClient.resolveByName({ structureType, name, createIfMissing: true })
  if (!entity) throw new Error(`resolveByName returned no entity for ${structureType} "${name}"`)
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

// --- Pure derivations over an entity array (consumers pass useAllEntities()) ---

function updatedAtMillis(entity: Entity): number {
  const ts = entity.updatedAt
  if (!ts) return 0
  return Number(ts.seconds) * 1000 + ts.nanos / 1_000_000
}

export function listByStructure(entities: Entity[], structureType: StructureType): Entity[] {
  const matches = entities.filter((e) => e.structureType === structureType)
  if (structureType === 'DailyNote') {
    return matches.sort((a, b) => b.name.localeCompare(a.name))
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
