import { useEffect } from 'react'
import type { WatchEntitiesResponse } from '@calcifer/proto/calcifer/v1/services_pb'
import { entityClient, qk, queryClient } from '~/model/api'
import { replica } from '~/model/replica'
import { writeRichTextIfNewer } from '~/model/richtext'

// The browser's copy of every entity is a replica fed only by
// EntityService.WatchEntities (ADR 9): each stream opens with a snapshot of every
// entity, then streams ordered events, and sends a fresh snapshot whenever the
// server can't guarantee continuity. The list is never fetched with `ListEntities`.
// Entities live in the store in `model/replica.ts`; rich-text docs stay in the
// TanStack cache. Mutations write the server's response through
// `replica.upsert` / `removeEntity`, the same path the events take; the Watch
// echo of that write changes nothing.

// The one removal path: Watch `deleted_id` and the browser's own delete.
export function removeEntity(id: string): void {
  replica.remove(id)
}

// Exported for tests.
export function applyWatchEvent({ event }: WatchEntitiesResponse): void {
  switch (event.case) {
    case 'snapshot':
      replica.applySnapshot(event.value.entities)
      // The set is current before any doc refetches. Snapshots carry no docs and
      // changes may have been missed (a reconnect or a lag), so refetch the open
      // ones. An editor with a local save pending or in flight doesn't take the
      // refetched version; its next save's conflict check covers it.
      void queryClient.invalidateQueries({ queryKey: qk.everyRichtext() })
      return
    case 'upserted':
      replica.upsert(event.value)
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
// the MCP agent's writes, without polling. A stream that fails or ends
// reconnects after a second (so a server that keeps closing streams can't cause
// a tight loop); the new stream's first message is a snapshot, which resyncs.
export function useEntitySync(): void {
  useEffect(() => {
    const controller = new AbortController()
    void (async () => {
      while (!controller.signal.aborted) {
        try {
          for await (const event of entityClient.watchEntities({}, { signal: controller.signal })) {
            if (controller.signal.aborted) return
            applyWatchEvent(event)
          }
          if (controller.signal.aborted) return
          console.warn('watch ended; reconnecting in 1s')
        } catch (err) {
          if (controller.signal.aborted) return
          console.warn('watch disconnected; reconnecting in 1s', err)
        }
        await new Promise((r) => setTimeout(r, 1000))
      }
    })()
    return () => controller.abort()
  }, [])
}
