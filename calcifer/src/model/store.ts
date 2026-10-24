import { useCallback, useEffect, useMemo, useRef } from 'react'
import type { ConnectError } from '@connectrpc/connect'
import { create as createMessage } from '@bufbuild/protobuf'
import { useMutation } from '@tanstack/react-query'
import { useShallow } from 'zustand/react/shallow'
import {
  EntityRefListSchema,
  EntityRefSchema,
  EntitySchema,
  PropertyValueSchema,
  type Entity,
  type EntityRef,
  type RichTextRef,
  type PropertyValue,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import type { ResolveEntityRequest } from '@calcifer/proto/calcifer/v1/services_pb'
import { entityClient } from '~/model/api'
import { timestampMsOrZero } from '~/model/dates'
import { fetchRichText, isRichTextEmpty, whenRichTextSaved } from '~/model/richtext'
import { replica, useReplica } from '~/model/replica'

export type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

// The entity RPCs wrap their entity in a response message; the server always
// sets it, so a missing one is a protocol error.
function requireEntity({ entity }: { entity?: Entity }): Entity {
  if (!entity) throw new Error('response carried no entity')
  return entity
}

// --- Queries ---

// The Watch replica (`model/replica.ts`): empty until the first snapshot arrives.
// One array per version of the set, so it is safe as a memo dependency.
export function useAllEntities(): Entity[] {
  return useReplica((s) => s.all)
}

// The entities of one structure, in replica order. Re-renders only when one of
// them changes, not on every write to the replica.
export function useEntitiesOfType(structureType: string): Entity[] {
  return useReplica(useShallow((s) => s.all.filter((e) => e.structureType === structureType)))
}

// The entities with `facet` attached, in replica order. Re-renders only when one
// of them changes.
export function useEntitiesWithFacet(facet: string): Entity[] {
  return useReplica(useShallow((s) => s.all.filter((e) => hasFacet(e, facet))))
}

// Each id's entity, undefined where the replica doesn't hold it. Re-renders only
// when one of them changes.
export function useEntitiesByIds(ids: readonly string[]): (Entity | undefined)[] {
  return useReplica(useShallow((s) => ids.map((id) => s.byId.get(id))))
}

// False until the first Watch snapshot has arrived.
export function useReplicaReady(): boolean {
  return useReplica((s) => s.ready)
}

export type EntityStatus = 'loading' | 'found' | 'missing'

export interface EntityLookup {
  entity: Entity | undefined
  status: EntityStatus
}

// One entity from the replica. `loading` until the first snapshot (unless an
// earlier write already cached it); `missing` once the replica is ready and
// doesn't hold the id, including right after it was deleted.
export function useEntity(id: string): EntityLookup {
  const entity = useReplica((s) => s.byId.get(id))
  const ready = useReplicaReady()
  const status: EntityStatus = entity ? 'found' : ready ? 'missing' : 'loading'
  return { entity, status }
}

// --- Mutations / actions ---

export interface CreateEntityOptions {
  name?: string
  // Facet ids to attach on create, with their select defaults.
  facets?: string[]
  // Initial values, laid over the defaults. Values on a facet's ids need that facet in `facets`.
  properties?: Record<string, PropertyValue>
}

interface CreateEntityVars extends CreateEntityOptions {
  structureType: string
}

// Create by intent: the server mints the id and builds the defaults, including
// the default name when `name` is omitted. Resolves to the saved entity.
export function useCreateEntity() {
  const { mutateAsync } = useMutation({
    mutationFn: async ({ structureType, name, facets, properties }: CreateEntityVars) =>
      requireEntity(await entityClient.createEntity({ structureType, name, facets, properties })),
    onSuccess: (entity) => replica.upsert(entity),
  })
  return useCallback(
    (structureType: string, options: CreateEntityOptions = {}) => mutateAsync({ structureType, ...options }),
    [mutateAsync],
  )
}

interface FacetVars {
  entityId: string
  facet: string
}

// Attach a facet through EntityService.AttachFacet. The server writes its select
// defaults; the returned entity replaces the cached one.
export function useAttachFacet() {
  const { mutateAsync } = useMutation({
    mutationFn: async ({ entityId, facet }: FacetVars) =>
      requireEntity(await entityClient.attachFacet({ entityId, facet })),
    onSuccess: (entity) => replica.upsert(entity),
  })
  return useCallback((entityId: string, facet: string) => mutateAsync({ entityId, facet }), [mutateAsync])
}

// Detach a facet through EntityService.DetachFacet, which deletes its values.
export function useDetachFacet() {
  const { mutateAsync } = useMutation({
    mutationFn: async ({ entityId, facet }: FacetVars) =>
      requireEntity(await entityClient.detachFacet({ entityId, facet })),
    onSuccess: (entity) => replica.upsert(entity),
  })
  return useCallback((entityId: string, facet: string) => mutateAsync({ entityId, facet }), [mutateAsync])
}

// Server-authoritative get-or-create through EntityService.ResolveEntity, so the
// browser and the MCP agent share one identity path. Writes the result into the replica.
async function resolveOrCreateEntity(structureType: string, key: ResolveEntityRequest['key']): Promise<Entity> {
  const entity = requireEntity(await entityClient.resolveEntity({ structureType, key, createIfMissing: true }))
  replica.upsert(entity)
  return entity
}

// Get-or-create the day's DailyNote. The server builds it and names it for the day.
export function useResolveDailyNote() {
  const { mutateAsync } = useMutation({
    mutationFn: (iso: string) => resolveOrCreateEntity('', { case: 'date', value: iso }),
  })
  return useCallback((iso: string) => mutateAsync(iso), [mutateAsync])
}

interface RenameEntityVars {
  id: string
  name: string
}

function renamed(entity: Entity, name: string): Entity {
  return createMessage(EntitySchema, { ...entity, name })
}

// Rename through EntityService.RenameEntity, which writes only the name, so it can't
// undo a concurrent property edit. Optimistic on `name`. On error only the name
// is restored, on the entity as cached now (the I-6 per-entity rollback), so a
// concurrent edit to anything else survives.
export function useRenameEntity() {
  const { mutate } = useMutation({
    mutationFn: async ({ id, name }: RenameEntityVars) => requireEntity(await entityClient.renameEntity({ id, name })),
    onMutate: ({ id, name }) => {
      const prevName = getEntity(id)?.name
      replica.patch(id, (e) => renamed(e, name))
      return { prevName }
    },
    onSuccess: (entity) => replica.upsert(entity),
    onError: (_err, { id }, ctx) => {
      const prev = ctx?.prevName
      if (prev !== undefined) replica.patch(id, (e) => renamed(e, prev))
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
// Optimistic, applied to the entity as currently cached (not the caller's
// possibly stale copy) so quick successive edits all show. On error only this
// property is restored, on the current cached entity: restoring a whole-entity
// snapshot would undo a concurrent edit to another property. On success the
// server's entity replaces the cached one, carrying anything the server derived
// from the write (e.g. a moved daily note's new name).
export function useSetProperty({ onError }: UseSetPropertyOptions = {}) {
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
    onMutate: ({ entity, propertyId, value }) => {
      const cached = getEntity(entity.id)
      // `null`: the property was absent. `undefined`: nothing cached to restore.
      const prevValue = cached && propertyValueOf(cached, propertyId)
      replica.patch(entity.id, (e) => withProperty(e, propertyId, value))
      return { prevValue }
    },
    onSuccess: (entity) => replica.upsert(entity),
    onError: (err, { entity, propertyId }, ctx) => {
      const prev = ctx?.prevValue
      if (prev !== undefined) replica.patch(entity.id, (e) => withProperty(e, propertyId, prev))
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

interface AddRelationTargetVars {
  entity: Entity
  propertyId: string
  target: RelationTarget
}

// Add one target to a `relations` property through EntityService.AddRelationTarget,
// which sends only that target, so it can't drop a target another writer added
// meanwhile (I-34). Optimistic, on the entity as currently cached. On error only
// this target is removed again, and only if the optimistic step added it, so a
// concurrent edit to another target survives. On success the server's entity
// replaces the cached one.
export function useAddRelationTarget({ onError }: UseSetPropertyOptions = {}) {
  // Latest callback without re-creating the mutation on every render.
  const onErrorRef = useRef(onError)
  useEffect(() => {
    onErrorRef.current = onError
  })
  const { mutate } = useMutation({
    mutationFn: async ({ entity, propertyId, target }: AddRelationTargetVars) =>
      requireEntity(await entityClient.addRelationTarget({ entityId: entity.id, propertyId, targetId: target.id })),
    onMutate: ({ entity, propertyId, target }) => {
      const cached = getEntity(entity.id)
      const added = cached !== undefined && withTargetAdded(cached, propertyId, target) !== cached
      replica.patch(entity.id, (e) => withTargetAdded(e, propertyId, target))
      return { added }
    },
    onSuccess: (entity) => replica.upsert(entity),
    onError: (err, { entity, propertyId, target }, ctx) => {
      if (ctx?.added) replica.patch(entity.id, (e) => withTargetRemoved(e, propertyId, target.id))
      onErrorRef.current?.(err, entity)
    },
  })
  return useCallback(
    (entity: Entity, propertyId: string, target: RelationTarget) => mutate({ entity, propertyId, target }),
    [mutate],
  )
}

interface RemoveRelationTargetVars {
  entity: Entity
  propertyId: string
  targetId: string
}

// Remove one target from a `relations` property through
// EntityService.RemoveRelationTarget, the counterpart of useAddRelationTarget.
// Optimistic, and removing the last target clears the property. On error only
// this target is added back, and only if the optimistic step removed it. On
// success the server's entity replaces the cached one.
export function useRemoveRelationTarget({ onError }: UseSetPropertyOptions = {}) {
  // Latest callback without re-creating the mutation on every render.
  const onErrorRef = useRef(onError)
  useEffect(() => {
    onErrorRef.current = onError
  })
  const { mutate } = useMutation({
    mutationFn: async ({ entity, propertyId, targetId }: RemoveRelationTargetVars) =>
      requireEntity(await entityClient.removeRelationTarget({ entityId: entity.id, propertyId, targetId })),
    onMutate: ({ entity, propertyId, targetId }) => {
      const cached = getEntity(entity.id)
      // `undefined`: the target wasn't there, so there is nothing to restore.
      const removed = cached && relationRefsOf(cached, propertyId).find((ref) => ref.id === targetId)
      replica.patch(entity.id, (e) => withTargetRemoved(e, propertyId, targetId))
      return { removed }
    },
    onSuccess: (entity) => replica.upsert(entity),
    onError: (err, { entity, propertyId }, ctx) => {
      const removed = ctx?.removed
      if (removed) replica.patch(entity.id, (e) => withTargetAdded(e, propertyId, removed))
      onErrorRef.current?.(err, entity)
    },
  })
  return useCallback(
    (entity: Entity, propertyId: string, targetId: string) => mutate({ entity, propertyId, targetId }),
    [mutate],
  )
}

export interface PropertyWriters {
  // `null` clears the text.
  setText: (entity: Entity, propertyId: string, text: string | null) => void
  // `null` clears the number.
  setNumber: (entity: Entity, propertyId: string, n: number | null) => void
  // `null` clears the date.
  setDate: (entity: Entity, propertyId: string, iso: string | null) => void
  setSelect: (entity: Entity, propertyId: string, key: string) => void
  // `null` clears the relation.
  setRelation: (entity: Entity, propertyId: string, target: RelationTarget | null) => void
  // Append one target; already present is a no-op.
  addRelationTarget: (entity: Entity, propertyId: string, target: RelationTarget) => void
  // Remove one target; absent is a no-op.
  removeRelationTarget: (entity: Entity, propertyId: string, targetId: string) => void
}

// Typed writes on top of useSetProperty, so callers never build a PropertyValue.
export function usePropertyWriters(options: UseSetPropertyOptions = {}): PropertyWriters {
  const setProperty = useSetProperty(options)
  const addRelationTarget = useAddRelationTarget(options)
  const removeRelationTarget = useRemoveRelationTarget(options)
  return useMemo(
    () => ({
      setText: (entity, propertyId, text) =>
        setProperty(entity, propertyId, text === null ? null : { case: 'text', value: text }),
      setNumber: (entity, propertyId, n) =>
        setProperty(entity, propertyId, n === null ? null : { case: 'number', value: n }),
      setDate: (entity, propertyId, iso) =>
        setProperty(entity, propertyId, iso === null ? null : { case: 'date', value: iso }),
      setSelect: (entity, propertyId, key) => setProperty(entity, propertyId, { case: 'select', value: key }),
      setRelation: (entity, propertyId, target) =>
        setProperty(
          entity,
          propertyId,
          target === null
            ? null
            : {
                case: 'relation',
                value: createMessage(EntityRefSchema, { id: target.id, structureType: target.structureType }),
              },
        ),
      addRelationTarget,
      removeRelationTarget,
    }),
    [setProperty, addRelationTarget, removeRelationTarget],
  )
}

// A relations property value, for a write or useCreateEntity's `properties`.
export function relationsValue(targets: readonly RelationTarget[]): PropertyValue {
  return createMessage(PropertyValueSchema, {
    value: {
      case: 'relations',
      value: createMessage(EntityRefListSchema, {
        refs: targets.map(({ id, structureType }) => ({ id, structureType })),
      }),
    },
  })
}

// --- Pure edit builders (the optimistic apply of useSetProperty() and the relation target writers) ---

// `null` removes the property. The server still clears links for declared
// relation properties that are absent (SetEntityProperty also for ad-hoc ones), so
// removing the last ref is safe.
function withProperty(entity: Entity, propertyId: string, value: PropertyValue['value'] | null): Entity {
  const properties = { ...entity.properties }
  if (value === null) delete properties[propertyId]
  else properties[propertyId] = createMessage(PropertyValueSchema, { value })
  return createMessage(EntitySchema, { ...entity, properties })
}

// A relations property's refs on `entity`. An absent property, or one that isn't
// `relations`, reads as an empty list.
function relationRefsOf(entity: Entity, propertyId: string): readonly EntityRef[] {
  const value = propertyValueOf(entity, propertyId)
  return value?.case === 'relations' ? value.value.refs : []
}

// Appends `target` to a relations property. Already present returns `entity` itself.
function withTargetAdded(entity: Entity, propertyId: string, target: RelationTarget): Entity {
  const refs = relationRefsOf(entity, propertyId)
  if (refs.some((ref) => ref.id === target.id)) return entity
  return withProperty(entity, propertyId, relationsValue([...refs, target]).value)
}

// Drops `targetId` from a relations property, deleting the property when the list
// empties. Not present returns `entity` itself.
function withTargetRemoved(entity: Entity, propertyId: string, targetId: string): Entity {
  const refs = relationRefsOf(entity, propertyId)
  if (!refs.some((ref) => ref.id === targetId)) return entity
  const rest = refs.filter((ref) => ref.id !== targetId)
  return withProperty(entity, propertyId, rest.length === 0 ? null : relationsValue(rest).value)
}

// --- Imperative (non-hook) helpers: delete, the editor mention flow, cleanup ---

export function getEntitiesSnapshot(): Entity[] {
  return useReplica.getState().all
}

export function getEntity(id: string): Entity | undefined {
  return useReplica.getState().byId.get(id)
}

export async function getOrCreateEntityForMention(
  structureType: string,
  name: string,
): Promise<{ id: string; name: string; structureType: string }> {
  // The server dedupes by (structureType, name).
  const entity = await resolveOrCreateEntity(structureType, { case: 'name', value: name })
  return { id: entity.id, name: entity.name, structureType: entity.structureType }
}

// The one delete path: fire-and-forget, dropped from the replica once the server confirms.
export function deleteEntity(id: string): void {
  void entityClient
    .deleteEntity({ id })
    .then(() => replica.remove(id))
    .catch((err) => console.error('delete failed', err))
}

// Deletes the entity only if the server's copy of `ref` is empty; reads the
// server, not the cache, so a stale cache can't discard an outside write.
// Local saves of `ref` (queued or in flight) land before the check, as long as
// they've begun: a caller leaving an editor runs this after its unmount flush
// (see useDailyNotePrune).
export function deleteEntityIfRichTextEmpty(id: string, ref: RichTextRef): void {
  void whenRichTextSaved(ref)
    .then(() => fetchRichText(ref))
    .then(({ doc }) => {
      // Small race left: a write landing between this Get and the Delete is lost.
      if (isRichTextEmpty(doc)) deleteEntity(id)
    })
    .catch((err) => console.error('empty check before delete failed', err))
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

export function hasFacet(entity: Entity, id: string): boolean {
  return entity.facets.includes(id)
}

export function dailyNoteDate(entity: Entity): string | undefined {
  if (entity.structureType !== 'DailyNote') return undefined
  const value = propertyValueOf(entity, 'date')
  return value?.case === 'date' ? value.value : undefined
}

export function dailyNoteByDate(entities: Entity[], iso: string): Entity | undefined {
  return entities.find((e) => dailyNoteDate(e) === iso)
}

export function entityUpdatedAtDate(entity: Entity): Date {
  return new Date(timestampMsOrZero(entity.updatedAt))
}
