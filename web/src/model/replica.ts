import { create } from 'zustand'
import { equals } from '@bufbuild/protobuf'
import { EntitySchema, type Entity } from 'calcifer/v1/entities_pb'
import { compareTimestamps } from '~/model/dates'

// The Watch-fed entity replica (ADR 9). Only model/sync.ts and the mutations
// in model/store.ts write it; components read it through store.ts's hooks.
// The Watch stream: `connecting` until the first attempt succeeds or fails, `live` from a
// stream's first message (always a snapshot), `retrying` from a failure or an ended stream
// until the next stream's first message. Set only by model/sync.ts.
export type ConnectionState = 'connecting' | 'live' | 'retrying'

interface ReplicaState {
  // Snapshot order, then creates; replacing a key keeps its slot, so lists such
  // as the mention menu don't reshuffle on an edit.
  byId: ReadonlyMap<string, Entity>
  // byId's values, rebuilt on every change: one stable array per version.
  all: Entity[]
  // False until this page load's first snapshot; never goes back.
  ready: boolean
  connection: ConnectionState
}

export const useReplica = create<ReplicaState>()(() => ({
  byId: new Map(),
  all: [],
  ready: false,
  connection: 'connecting',
}))

// Ids deleted this session. Ids are server-minted UUIDs and never reused, so this is never cleared.
const tombstones = new Set<string>()

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
  // Replaces the whole set. A snapshot is authoritative except for deletes made
  // this session: a resync generated before the server committed one would
  // bring the entity back, so tombstoned ids are dropped. Timestamps aren't checked.
  applySnapshot(entities: readonly Entity[]): void {
    const { byId: prev, ready } = useReplica.getState()
    const live = entities.filter((e) => !tombstones.has(e.id))
    const byId = new Map(live.map((e) => [e.id, reuse(prev.get(e.id), e)]))
    if (ready && sameEntries(prev, byId)) return
    commit(byId, true)
  },
  // A server-sourced entity (Watch event or mutation response). Dropped when it
  // was deleted this session or is older than the cached copy (I-49). Equal
  // timestamps are accepted: two writes can land in the same millisecond.
  upsert(entity: Entity): void {
    if (tombstones.has(entity.id)) return
    const cached = useReplica.getState().byId.get(entity.id)
    if (cached && compareTimestamps(entity.updatedAt, cached.updatedAt) < 0) return
    put(entity)
  },
  remove(id: string): void {
    tombstones.add(id)
    const { byId: prev, ready } = useReplica.getState()
    if (!prev.has(id)) return
    const byId = new Map(prev)
    byId.delete(id)
    commit(byId, ready)
  },
  // Optimistic edit of the cached entity. Not guarded. Does nothing if it isn't cached.
  patch(id: string, edit: (entity: Entity) => Entity): void {
    const cached = useReplica.getState().byId.get(id)
    if (cached) put(edit(cached))
  },
  // The Watch stream's state. Touches nothing else, and nothing at all when unchanged.
  setConnection(connection: ConnectionState): void {
    if (useReplica.getState().connection === connection) return
    useReplica.setState({ connection })
  },
  // Test-only: back to an empty, not-ready replica with no deletes.
  reset(): void {
    tombstones.clear()
    useReplica.setState(useReplica.getInitialState(), true)
  },
}
