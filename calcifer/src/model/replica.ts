import { create } from 'zustand'
import { equals } from '@bufbuild/protobuf'
import { EntitySchema, type Entity } from '@calcifer/proto/calcifer/v1/entities_pb'

// The Watch-fed entity replica (ADR 9). Only model/sync.ts and the mutations
// in model/store.ts write it; components read it through store.ts's hooks.
interface ReplicaState {
  // Snapshot order, then creates; replacing a key keeps its slot, so lists such
  // as the mention menu don't reshuffle on an edit.
  byId: ReadonlyMap<string, Entity>
  // byId's values, rebuilt on every change: one stable array per version.
  all: Entity[]
  // False until this page load's first snapshot; never goes back.
  ready: boolean
}

export const useReplica = create<ReplicaState>()(() => ({ byId: new Map(), all: [], ready: false }))

function commit(byId: Map<string, Entity>, ready: boolean): void {
  useReplica.setState({ byId, all: [...byId.values()], ready })
}

// An unchanged entity keeps its cached object, so a Watch echo or an unchanged
// entity in a resync snapshot changes no reference and re-renders nothing.
function reuse(cached: Entity | undefined, next: Entity): Entity {
  return cached && equals(EntitySchema, cached, next) ? cached : next
}

function sameEntries(a: ReadonlyMap<string, Entity>, b: ReadonlyMap<string, Entity>): boolean {
  if (a.size !== b.size) return false
  for (const [id, entity] of b) if (a.get(id) !== entity) return false
  return true
}

function put(entity: Entity): void {
  const { byId, ready } = useReplica.getState()
  const cached = byId.get(entity.id)
  if (reuse(cached, entity) === cached) return
  commit(new Map(byId).set(entity.id, entity), ready)
}

export const replica = {
  // Replaces the whole set. A snapshot is authoritative.
  applySnapshot(entities: readonly Entity[]): void {
    const { byId: prev, ready } = useReplica.getState()
    const byId = new Map(entities.map((e) => [e.id, reuse(prev.get(e.id), e)]))
    if (ready && sameEntries(prev, byId)) return
    commit(byId, true)
  },
  // A server-sourced entity (Watch event or mutation response).
  upsert(entity: Entity): void {
    put(entity)
  },
  remove(id: string): void {
    const { byId: prev, ready } = useReplica.getState()
    if (!prev.has(id)) return
    const byId = new Map(prev)
    byId.delete(id)
    commit(byId, ready)
  },
  // Optimistic edit of the cached entity. Does nothing if it isn't cached.
  patch(id: string, edit: (entity: Entity) => Entity): void {
    const cached = useReplica.getState().byId.get(id)
    if (cached) put(edit(cached))
  },
}
