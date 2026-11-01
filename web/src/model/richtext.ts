import { useCallback, useEffect, useRef, useState } from 'react'
import { create as createMessage } from '@bufbuild/protobuf'
import { TimestampSchema, type Timestamp } from '@bufbuild/protobuf/wkt'
import { Code, ConnectError } from '@connectrpc/connect'
import { useQuery } from '@tanstack/react-query'
import { RichTextRefSchema, type RichText, type RichTextRef } from 'calcifer/v1/entities_pb'
import { qk, queryClient, richTextClient } from '~/model/api'
import { compareTimestamps } from '~/model/dates'
import { useReplica } from '~/model/replica'

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
  text?: string
  content?: RichTextDocNode[]
}

// A doc's text with its blocks run together, for a preview. Mentions and other
// atoms without text are left out.
export function richTextPlainText(doc: string | undefined): string {
  if (!doc) return ''
  let parsed: RichTextDocNode
  try {
    parsed = JSON.parse(doc) as RichTextDocNode
  } catch {
    return ''
  }
  const blocks: string[] = []
  const walk = (node: RichTextDocNode): string => node.text ?? (node.content ?? []).map(walk).join('')
  for (const block of parsed.content ?? []) {
    const text = walk(block).trim()
    if (text) blocks.push(text)
  }
  return blocks.join(' ')
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

export function isNewerRichText(a: Timestamp | undefined, b: Timestamp | undefined): boolean {
  return compareTimestamps(a, b) > 0
}

// The rich-text RPCs wrap their doc in a response message; the server always
// sets it, so a missing one is a protocol error.
function requireRichText({ richText }: { richText?: RichText }): RichText {
  if (!richText) throw new Error('response carried no rich text')
  return richText
}

function toState(rt: RichText): RichTextState {
  return { doc: rt.doc, updatedAt: rt.updatedAt }
}

// Reads the doc from the server, bypassing the cache. A declared but unsaved
// doc comes back as '' at the epoch; any error (NotFound included) is thrown.
export async function fetchRichText(ref: RichTextRef): Promise<RichTextState> {
  const rt = requireRichText(await richTextClient.getRichText({ entityId: ref.entityId, propertyId: ref.propertyId }))
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
  const key = qk.richtext(ref.entityId, ref.propertyId)
  return useQuery({
    queryKey: key,
    // Keeps the cached version unless the fetched one is newer: a Watch event
    // or Put response can land while the fetch is in flight (I-49). Equal keeps
    // the cached object, so nothing re-renders.
    queryFn: async () => {
      const fetched = await fetchRichText(ref)
      const cached = queryClient.getQueryData<RichTextState>(key)
      return cached && !isNewerRichText(fetched.updatedAt, cached.updatedAt) ? cached : fetched
    },
    enabled: !!ref.entityId,
  })
}

export function getRichTextSnapshot(ref: RichTextRef): RichTextState | undefined {
  return queryClient.getQueryData<RichTextState>(qk.richtext(ref.entityId, ref.propertyId))
}

function isFailedPrecondition(err: unknown): boolean {
  return err instanceof ConnectError && err.code === Code.FailedPrecondition
}

// A save failure worth retrying: anything but the codes where the same doc can
// never succeed. Network failures that aren't a ConnectError count as retryable.
// FailedPrecondition (a conflict) is handled before this is asked.
function isRetryable(err: unknown): boolean {
  if (!(err instanceof ConnectError)) return true
  return ![Code.InvalidArgument, Code.NotFound, Code.PermissionDenied, Code.Unimplemented].includes(err.code)
}

// Resolves after the backoff for the `failures`-th failure in a row: 1 s,
// doubling, capped at 30 s. Resolves early when the Watch stream turns `live`,
// since the server was just reached.
function waitToRetry(failures: number): Promise<void> {
  const delay = Math.min(1_000 * 2 ** (failures - 1), 30_000)
  return new Promise((resolve) => {
    const finish = () => {
      clearTimeout(timer)
      unsubscribe()
      resolve()
    }
    const timer = setTimeout(finish, delay)
    // Only the change to `live`: the replica also changes while it stays live.
    const unsubscribe = useReplica.subscribe((state, prev) => {
      if (state.connection === 'live' && prev.connection !== 'live') finish()
    })
  })
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
  // Queue a save of `doc`, built on the server version `base` (the saver's own
  // latest if omitted). Saves are serialised: while one is in flight only the
  // latest queued doc is kept, with its base, and it goes out once the in-flight
  // one resolves. It expects the newer of its base and the version this saver's
  // own Puts produced, so a doc built on a version before an outside change
  // conflicts instead of landing. A save that fails is retried with backoff
  // until it lands (see `usePutRichText`).
  save: (doc: string, base?: Timestamp) => void
  // The caller's content now reflects `updatedAt` (e.g. an outside change was
  // loaded into the editor), so a save that doesn't name its base expects it.
  adopt: (updatedAt: Timestamp | undefined) => void
  // True while a Put is in flight or a doc is queued behind one.
  busy: boolean
  // True from a save that failed and is being retried until a save succeeds (or a conflict
  // or a dropped error ends it).
  unsaved: boolean
}

// A doc waiting to be sent and the server version it was built on.
interface PendingSave {
  doc: string
  base: Timestamp | undefined
}

interface SaveQueue {
  // The version a save that names no base is built on.
  base: Timestamp | undefined
  // The newest version this saver's own Puts produced. A doc built on an older
  // version of the same editor content expects this, not the older one.
  ownBase: Timestamp | undefined
  inFlight: boolean
  pending: PendingSave | null
  // Failed attempts in a row, for the backoff. Zero after a save lands or is given up on.
  failures: number
}

interface SaveTarget {
  ref: RichTextRef
  onSaved?: (saved: RichTextState) => void
  onConflict?: (latest: RichTextState) => void
}

// `saved`: the Put landed. `retry`: it failed in a way that may pass, so the doc
// is back in `queue.pending` (unless a newer one is queued) and the caller backs
// off. `dropped`: it can never succeed, so it was logged and discarded.
// `conflict`: it lost to another writer, so queued docs were dropped and the
// latest version reloaded.
type SaveOutcome = 'saved' | 'retry' | 'dropped' | 'conflict'

function newerOf(a: Timestamp | undefined, b: Timestamp | undefined): Timestamp | undefined {
  return isNewerRichText(b, a) ? b : a
}

async function putOnce(
  queue: SaveQueue,
  sent: PendingSave,
  { ref, onSaved, onConflict }: SaveTarget,
): Promise<SaveOutcome> {
  try {
    const saved = requireRichText(
      await richTextClient.putRichText({
        entityId: ref.entityId,
        propertyId: ref.propertyId,
        doc: sent.doc,
        expectedUpdatedAt: newerOf(sent.base, queue.ownBase) ?? EPOCH,
      }),
    )
    queue.base = saved.updatedAt
    queue.ownBase = saved.updatedAt
    queue.failures = 0
    writeRichTextIfNewer(saved)
    // Put also changed the entity's links, referenced_dates and updated_at on
    // the server; its Watch `upserted` event brings them into the replica.
    onSaved?.(toState(saved))
    return 'saved'
  } catch (err) {
    if (isFailedPrecondition(err)) {
      // Lost to another writer: drop queued local docs and reload (no merge).
      queue.failures = 0
      queue.pending = null
      await reloadAfterConflict(queue, ref, onConflict)
      return 'conflict'
    }
    if (isRetryable(err)) {
      // Keep the doc unless a newer one was queued while this Put was in flight.
      queue.pending ??= sent
      queue.failures += 1
      return 'retry'
    }
    console.error('rich text save failed', err)
    queue.failures = 0
    return 'dropped'
  }
}

async function reloadAfterConflict(
  queue: SaveQueue,
  ref: RichTextRef,
  onConflict: SaveTarget['onConflict'],
): Promise<void> {
  try {
    const latest = requireRichText(
      await richTextClient.getRichText({ entityId: ref.entityId, propertyId: ref.propertyId }),
    )
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
// = the version its doc is built on (the epoch if nothing is saved), so a save
// built on a stale version fails instead of overwriting an outside change.
// A Put that fails for a reason that may pass (the server is unreachable, say)
// is retried forever, backing off 1 s doubling to 30 s and going at once when
// the Watch stream turns live, always with the newest queued doc. The doc keeps
// its outstanding save meanwhile, so `whenRichTextSaved` waits for it to land.
// Only a conflict or an error the doc can never get past ends the retrying.
export function usePutRichText(
  ref: RichTextRef,
  { initialBase, onSaved, onConflict }: UsePutRichTextOptions,
): RichTextSaver {
  const [busy, setBusy] = useState(false)
  const [unsaved, setUnsaved] = useState(false)
  const queueRef = useRef<SaveQueue>({
    base: initialBase,
    ownBase: undefined,
    inFlight: false,
    pending: null,
    failures: 0,
  })
  // Latest ref and callbacks without re-creating `save` on every render.
  const targetRef = useRef<SaveTarget>({ ref, onSaved, onConflict })
  useEffect(() => {
    targetRef.current = { ref, onSaved, onConflict }
  })

  const save = useCallback((doc: string, base?: Timestamp) => {
    const queue = queueRef.current
    queue.pending = { doc, base: base ?? queue.base }
    if (queue.inFlight) return
    queue.inFlight = true
    setBusy(true)
    const key = richTextKey(targetRef.current.ref)
    beginSave(key)
    void (async () => {
      while (queue.pending !== null) {
        const next = queue.pending
        queue.pending = null
        const outcome = await putOnce(queue, next, targetRef.current)
        setUnsaved(outcome === 'retry')
        if (outcome === 'retry') await waitToRetry(queue.failures)
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

  return { save, adopt, busy, unsaved }
}
