import { useCallback, useEffect, useRef, useState } from 'react'
import { create as createMessage } from '@bufbuild/protobuf'
import { TimestampSchema, type Timestamp } from '@bufbuild/protobuf/wkt'
import { Code, ConnectError } from '@connectrpc/connect'
import { useQuery } from '@tanstack/react-query'
import {
  RichTextRefSchema,
  RichTextSchema,
  type RichText,
  type RichTextRef,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { qk, queryClient, richTextClient } from '~/model/api'

// The address of one rich-text doc: an entity and one of its structure's
// declared rich-text property ids (see `richTextPropertyIds`). Entities store
// no value for these properties (ADR 8), so the ref is always derived.
export function richTextRef(entityId: string, propertyId: string): RichTextRef {
  return createMessage(RichTextRefSchema, { entityId, propertyId })
}

export function richTextKey(ref: RichTextRef): string {
  return `${ref.entityId}:${ref.propertyId}`
}

// What `qk.richtext` holds: the doc JSON string ('' when nothing is saved) and
// the server's `updated_at` for it (the epoch when nothing is saved).
export interface RichTextState {
  doc: string
  updatedAt: Timestamp | undefined
}

interface RichTextDocNode {
  type?: string
  content?: RichTextDocNode[]
}

export function isRichTextEmpty(doc: string | undefined): boolean {
  if (!doc) return true
  let parsed: RichTextDocNode
  try {
    parsed = JSON.parse(doc) as RichTextDocNode
  } catch {
    return false
  }
  const content = parsed.content
  if (!content || content.length === 0) return true
  if (content.length > 1) return false
  const only = content[0]
  if (only.type !== 'paragraph') return false
  return !only.content || only.content.length === 0
}

// Unset reads as the epoch, which is what the server reports for "nothing saved".
function compareTimestamps(a: Timestamp | undefined, b: Timestamp | undefined): number {
  const as = a?.seconds ?? 0n
  const bs = b?.seconds ?? 0n
  if (as !== bs) return as < bs ? -1 : 1
  return (a?.nanos ?? 0) - (b?.nanos ?? 0)
}

export function isNewerRichText(a: Timestamp | undefined, b: Timestamp | undefined): boolean {
  return compareTimestamps(a, b) > 0
}

function toState(rt: RichText): RichTextState {
  return { doc: rt.doc, updatedAt: rt.updatedAt }
}

// Reads the doc from the server, bypassing the cache. A declared but unsaved
// doc comes back as '' at the epoch; any error (NotFound included) is thrown.
export async function fetchRichText(ref: RichTextRef): Promise<RichTextState> {
  const rt = await richTextClient.get({ entityId: ref.entityId, propertyId: ref.propertyId })
  return toState(rt)
}

// Writes a saved RichText (Put response or Watch `rich_text_changed`) to the
// cache unless the cache already holds the same or a newer version.
export function writeRichTextIfNewer(rt: RichText): void {
  const ref = rt.ref
  if (!ref) return
  const key = qk.richtext(ref.entityId, ref.propertyId)
  const cached = queryClient.getQueryData<RichTextState>(key)
  if (cached && !isNewerRichText(rt.updatedAt, cached.updatedAt)) return
  queryClient.setQueryData<RichTextState>(key, toState(rt))
}

export function useRichText(ref: RichTextRef) {
  return useQuery({
    queryKey: qk.richtext(ref.entityId, ref.propertyId),
    queryFn: () => fetchRichText(ref),
    enabled: !!ref.entityId,
  })
}

export function getRichTextSnapshot(ref: RichTextRef): RichTextState | undefined {
  return queryClient.getQueryData<RichTextState>(qk.richtext(ref.entityId, ref.propertyId))
}

function isFailedPrecondition(err: unknown): boolean {
  return err instanceof ConnectError && err.code === Code.FailedPrecondition
}

const EPOCH = createMessage(TimestampSchema, { seconds: 0n, nanos: 0 })

export interface UsePutRichTextOptions {
  // The version the caller's content is based on when the hook mounts.
  initialBase: Timestamp | undefined
  // A Put succeeded; `saved` is the server's response.
  onSaved?: (saved: RichTextState) => void
  // A Put lost to another writer. `latest` is the server's current version,
  // already written to the cache; queued local docs have been dropped.
  onConflict?: (latest: RichTextState) => void
}

export interface RichTextSaver {
  // Queue a save of `doc`. Saves are serialised: while one is in flight only
  // the latest queued doc is kept, and it goes out once the in-flight one
  // resolves, expecting the version that one produced.
  save: (doc: string) => void
  // The caller's content now reflects `updatedAt` (e.g. an outside change was
  // loaded into the editor), so the next save expects it.
  adopt: (updatedAt: Timestamp | undefined) => void
  // True while a Put is in flight or a doc is queued behind one.
  busy: boolean
}

interface SaveQueue {
  // The version the next Put expects (`expected_updated_at`).
  base: Timestamp | undefined
  inFlight: boolean
  pending: string | null
}

interface SaveTarget {
  ref: RichTextRef
  onSaved?: (saved: RichTextState) => void
  onConflict?: (latest: RichTextState) => void
}

async function putOnce(queue: SaveQueue, doc: string, { ref, onSaved, onConflict }: SaveTarget): Promise<void> {
  try {
    const saved = await richTextClient.put(
      createMessage(RichTextSchema, { ref, doc, expectedUpdatedAt: queue.base ?? EPOCH }),
    )
    queue.base = saved.updatedAt
    writeRichTextIfNewer(saved)
    // Put derived this entity's links + referenced_dates server-side, which
    // also changes targets' backlinks — refresh the source entity and the
    // list that backlinks/calendar derive from.
    void queryClient.invalidateQueries({ queryKey: qk.entity(ref.entityId) })
    void queryClient.invalidateQueries({ queryKey: ['entities'] })
    onSaved?.(toState(saved))
  } catch (err) {
    if (!isFailedPrecondition(err)) {
      console.error('rich text save failed', err)
      return
    }
    // Lost to another writer: drop queued local docs and reload (no merge).
    queue.pending = null
    await reloadAfterConflict(queue, ref, onConflict)
  }
}

async function reloadAfterConflict(
  queue: SaveQueue,
  ref: RichTextRef,
  onConflict: SaveTarget['onConflict'],
): Promise<void> {
  try {
    const latest = await richTextClient.get({ entityId: ref.entityId, propertyId: ref.propertyId })
    queue.base = latest.updatedAt
    writeRichTextIfNewer(latest)
    onConflict?.(toState(latest))
  } catch (err) {
    // The base stays stale, so the next save conflicts again and retries the reload.
    console.error('rich text reload after conflict failed', err)
  }
}

// Outstanding saves per doc (`richTextKey`): how many editors' save queues have
// a Put in flight or a doc queued, and who is waiting for that to reach zero.
const outstandingSaves = new Map<string, number>()
const savedWaiters = new Map<string, Array<() => void>>()

function beginSave(key: string): void {
  outstandingSaves.set(key, (outstandingSaves.get(key) ?? 0) + 1)
}

function endSave(key: string): void {
  const count = (outstandingSaves.get(key) ?? 1) - 1
  if (count > 0) {
    outstandingSaves.set(key, count)
    return
  }
  outstandingSaves.delete(key)
  const waiters = savedWaiters.get(key) ?? []
  savedWaiters.delete(key)
  for (const resolve of waiters) resolve()
}

// Resolves once the doc has no save in flight or queued (at once if it has none).
export function whenRichTextSaved(ref: RichTextRef): Promise<void> {
  const key = richTextKey(ref)
  if (!outstandingSaves.has(key)) return Promise.resolve()
  return new Promise((resolve) => {
    savedWaiters.set(key, [...(savedWaiters.get(key) ?? []), resolve])
  })
}

// Conditional saves for one rich-text doc. Each Put sends `expected_updated_at`
// = the version the content is based on (the epoch if nothing is saved), so a
// save built on a stale version fails instead of overwriting an outside change.
export function usePutRichText(
  ref: RichTextRef,
  { initialBase, onSaved, onConflict }: UsePutRichTextOptions,
): RichTextSaver {
  const [busy, setBusy] = useState(false)
  const queueRef = useRef<SaveQueue>({ base: initialBase, inFlight: false, pending: null })
  // Latest ref and callbacks without re-creating `save` on every render.
  const targetRef = useRef<SaveTarget>({ ref, onSaved, onConflict })
  useEffect(() => {
    targetRef.current = { ref, onSaved, onConflict }
  })

  const save = useCallback((doc: string) => {
    const queue = queueRef.current
    queue.pending = doc
    if (queue.inFlight) return
    queue.inFlight = true
    setBusy(true)
    const key = richTextKey(targetRef.current.ref)
    beginSave(key)
    void (async () => {
      while (queue.pending !== null) {
        const next = queue.pending
        queue.pending = null
        await putOnce(queue, next, targetRef.current)
      }
      queue.inFlight = false
      setBusy(false)
      endSave(key)
    })()
  }, [])

  const adopt = useCallback((updatedAt: Timestamp | undefined) => {
    const queue = queueRef.current
    if (isNewerRichText(updatedAt, queue.base)) queue.base = updatedAt
  }, [])

  return { save, adopt, busy }
}
