import { useEffect } from 'react'
import { queryOptions } from '@tanstack/react-query'
import type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'
import type { EntityEvent } from '@calcifer/proto/calcifer/v1/services_pb'
import { entityClient, qk, queryClient } from '~/model/api'
import { writeRichTextIfNewer } from '~/model/richtext'

// The browser's copy of every entity is a replica fed only by
// EntityService.Watch (ADR 9): each stream opens with a snapshot of every
// entity, then streams ordered events, and sends a fresh snapshot whenever the
// server can't guarantee continuity. The list is never fetched with `List` and
// never invalidated. Mutations write the server's response through
// `writeEntity` / `removeEntity`, the same path the events take; the Watch echo
// of that write is harmless because applying an upsert is idempotent.

let resolveFirstSnapshot: (entities: Entity[]) => void = () => {}
const firstSnapshot = new Promise<Entity[]>((resolve) => {
  resolveFirstSnapshot = resolve
})

// Every entity. Resolves with the first Watch snapshot; after that the Watch
// loop keeps it current, so it is never stale and never refetched.
export const entitiesQuery = queryOptions({
  queryKey: qk.entities(),
  queryFn: () => firstSnapshot,
  staleTime: Infinity,
  gcTime: Infinity,
})

// Insert or replace one entity in the list and its detail cache. Before the
// first snapshot there is no list yet, and the snapshot will include it.
export function writeEntity(entity: Entity): void {
  queryClient.setQueryData(qk.entity(entity.id), entity)
  queryClient.setQueryData<Entity[]>(qk.entities(), (list) => {
    if (!list) return list
    const at = list.findIndex((e) => e.id === entity.id)
    if (at === -1) return [...list, entity]
    return list.map((e, i) => (i === at ? entity : e))
  })
}

export function removeEntity(id: string): void {
  queryClient.removeQueries({ queryKey: qk.entity(id), exact: true })
  queryClient.setQueryData<Entity[]>(qk.entities(), (list) => list?.filter((e) => e.id !== id))
}

async function applySnapshot(entities: Entity[]): Promise<void> {
  if (queryClient.getQueryData(qk.entities()) === undefined) {
    // First snapshot: hand it to entitiesQuery's fetch (a component may
    // already be waiting on it) and wait until it is in the cache, so no later
    // event is applied before that fetch lands and overwrites it.
    resolveFirstSnapshot(entities)
    await queryClient.fetchQuery(entitiesQuery)
  } else {
    queryClient.setQueryData(qk.entities(), entities)
  }

  const byId = new Map(entities.map((e) => [e.id, e]))
  for (const query of queryClient.getQueryCache().findAll({ queryKey: qk.everyEntity() })) {
    const id = query.queryKey[1] as string
    const entity = byId.get(id)
    if (entity) queryClient.setQueryData(qk.entity(id), entity)
    else queryClient.removeQueries({ queryKey: qk.entity(id), exact: true })
  }

  // Snapshots carry no rich-text docs, and changes may have been missed
  // (a reconnect or a lag), so refetch the open docs. An editor with a local
  // save pending or in flight doesn't take the refetched version; its next
  // save's conflict check covers it.
  void queryClient.invalidateQueries({ queryKey: qk.everyRichtext() })
}

async function applyEvent({ event }: EntityEvent): Promise<void> {
  switch (event.case) {
    case 'snapshot':
      await applySnapshot(event.value.entities)
      return
    case 'upserted':
      writeEntity(event.value)
      return
    case 'deletedId':
      removeEntity(event.value)
      return
    case 'richTextChanged':
      writeRichTextIfNewer(event.value)
      return
  }
}

// Long-lived Watch loop: keeps the replica in step with the server, including
// the MCP agent's writes, without polling. A dropped stream reconnects after a
// second; the new stream's first message is a snapshot, which resyncs.
export function useEntitySync(): void {
  useEffect(() => {
    const controller = new AbortController()
    void (async () => {
      while (!controller.signal.aborted) {
        try {
          for await (const event of entityClient.watch({}, { signal: controller.signal })) {
            if (controller.signal.aborted) return
            await applyEvent(event)
          }
        } catch (err) {
          if (controller.signal.aborted) return
          console.warn('watch disconnected; reconnecting in 1s', err)
          await new Promise((r) => setTimeout(r, 1000))
        }
      }
    })()
    return () => controller.abort()
  }, [])
}
