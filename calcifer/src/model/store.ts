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
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { STRUCTURES, hasUniqueNames, type StructureType } from '~/model/structures'
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

export function useRenameEntity() {
  const qc = useQueryClient()
  const m = useMutation({
    mutationFn: (entity: Entity) => entityClient.update({ entity }),
    onMutate: async (entity) => {
      await qc.cancelQueries({ queryKey: qk.entity(entity.id) })
      const prev = qc.getQueryData<Entity>(qk.entity(entity.id))
      qc.setQueryData(qk.entity(entity.id), entity)
      return { prev }
    },
    onError: (_e, entity, ctx) => {
      if (ctx?.prev) qc.setQueryData(qk.entity(entity.id), ctx.prev)
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ['entities'] })
    },
  })
  return useCallback(
    (current: Entity, name: string) => {
      m.mutate(createMessage(EntitySchema, { ...current, name }))
    },
    [m],
  )
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

export function useMoveDailyNote() {
  const m = useMutation({
    mutationFn: (entity: Entity) => entityClient.update({ entity }),
    onSuccess: onEntityWritten,
  })
  return useCallback(
    (entity: Entity, newIso: string): boolean => {
      const dateProp = entity.properties.find((p) => p.id === 'date')
      const v = dateProp?.value?.value
      if (v?.case === 'date' && v.value === newIso) return true
      const occupant = dailyNoteByDate(getEntitiesSnapshot(), newIso)
      if (occupant && occupant.id !== entity.id) return false
      const properties = entity.properties.map((p) =>
        p.id === 'date'
          ? createMessage(PropertySchema, {
              id: 'date',
              value: createMessage(PropertyValueSchema, { value: { case: 'date', value: newIso } }),
            })
          : p,
      )
      m.mutate(createMessage(EntitySchema, { ...entity, name: formatLongDate(newIso), properties }))
      return true
    },
    [m],
  )
}

// --- Imperative (non-hook) helpers for the editor mention flow + cleanup ---

export function getEntitiesSnapshot(): Entity[] {
  return queryClient.getQueryData<Entity[]>(qk.entities()) ?? []
}

export function getOrCreateEntityForMention(
  structureType: CreatableStructureType,
  name: string,
): { id: string; name: string; structureType: string } {
  if (hasUniqueNames(structureType)) {
    const lower = name.toLowerCase()
    const existing = getEntitiesSnapshot().find(
      (e) => e.structureType === structureType && e.name.toLowerCase() === lower,
    )
    if (existing) return { id: existing.id, name: existing.name, structureType: existing.structureType }
  }
  // Fire-and-forget create; the client-minted id makes the chip valid immediately.
  const entity = buildEntityMessage(structureType, name)
  void entityClient
    .create({ entity })
    .then(onEntityWritten)
    .catch((err) => console.error('mention create failed', err))
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

export function dailyNoteByDate(entities: Entity[], iso: string): Entity | undefined {
  for (const entity of entities) {
    if (entity.structureType !== 'DailyNote') continue
    const dateProp = entity.properties.find((p) => p.id === 'date')
    const value = dateProp?.value?.value
    if (value?.case === 'date' && value.value === iso) return entity
  }
  return undefined
}

export function entitiesByDate(entities: Entity[], iso: string): Entity[] {
  return entities.filter((e) => e.referencedDates.includes(iso))
}

export function daysWithContent(entities: Entity[]): Set<string> {
  const set = new Set<string>()
  for (const e of entities) {
    if (e.structureType === 'DailyNote') {
      const dateProp = e.properties.find((p) => p.id === 'date')
      const v = dateProp?.value?.value
      if (v?.case === 'date') set.add(v.value)
    }
    for (const iso of e.referencedDates) set.add(iso)
  }
  return set
}

export function entityUpdatedAtDate(entity: Entity): Date {
  return new Date(updatedAtMillis(entity))
}
