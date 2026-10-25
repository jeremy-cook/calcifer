import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { WatchEntitiesResponse } from '@calcifer/proto/calcifer/v1/services_pb'
import type { ConnectionState } from '~/model/replica'
import { entity, richText, watch } from '~/model/testing'

// Fresh modules per test: the replica and its caches are module state.
let api: typeof import('~/model/api')
let store: typeof import('~/model/store')
let sync: typeof import('~/model/sync')

beforeEach(async () => {
  vi.resetModules()
  api = await import('~/model/api')
  store = await import('~/model/store')
  sync = await import('~/model/sync')
})

const ids = () => store.getEntitiesSnapshot().map((e) => e.id)

describe('applyWatchEvent', () => {
  it('replaces the set with a snapshot, dropping entities it omits', () => {
    sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    sync.applyWatchEvent(watch.snapshot(entity('b'), entity('c')))
    expect(ids()).toEqual(['b', 'c'])
  })

  it('appends an upserted new id', () => {
    sync.applyWatchEvent(watch.snapshot(entity('a')))
    sync.applyWatchEvent(watch.upserted(entity('b')))
    expect(ids()).toEqual(['a', 'b'])
  })

  it('replaces an upserted existing id in place', () => {
    sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b'), entity('c')))
    sync.applyWatchEvent(watch.upserted(entity('b', 'renamed', 2_000)))
    expect(ids()).toEqual(['a', 'b', 'c'])
    expect(store.getEntitiesSnapshot()[1].name).toBe('renamed')
  })

  it('removes a deleted id', () => {
    sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    sync.applyWatchEvent(watch.deletedId('a'))
    expect(ids()).toEqual(['b'])
  })

  it('lets a later snapshot win over earlier events', () => {
    sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    sync.applyWatchEvent(watch.upserted(entity('c')))
    sync.applyWatchEvent(watch.upserted(entity('a', 'renamed', 2_000)))
    sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    expect(store.getEntitiesSnapshot()).toEqual([entity('a'), entity('b')])
  })

  it('holds the snapshot before it invalidates rich text', () => {
    const seen: string[][] = []
    vi.spyOn(api.queryClient, 'invalidateQueries').mockImplementation(async () => {
      seen.push(ids())
    })
    sync.applyWatchEvent(watch.snapshot(entity('a')))
    sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    expect(seen).toEqual([['a'], ['a', 'b']])
  })

  it('writes a changed rich-text doc to its cache', () => {
    sync.applyWatchEvent(watch.richTextChanged(richText('a', 'content', '{"v":1}', 2_000)))
    expect(api.queryClient.getQueryData(api.qk.richtext('a', 'content'))).toMatchObject({ doc: '{"v":1}' })
  })
})

describe('runEntitySync', () => {
  type WatchEntities = (request: unknown, options: { signal: AbortSignal }) => AsyncIterable<WatchEntitiesResponse>

  let useReplica: typeof import('~/model/replica').useReplica
  let watchEntities: ReturnType<typeof vi.fn<WatchEntities>>

  // Fresh modules again, now with the Watch client mocked out.
  beforeEach(async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    watchEntities = vi.fn<WatchEntities>()
    vi.resetModules()
    vi.doMock('~/model/api', async (importOriginal) => ({
      ...(await importOriginal<typeof import('~/model/api')>()),
      entityClient: { watchEntities },
    }))
    ;({ useReplica } = await import('~/model/replica'))
    sync = await import('~/model/sync')
  })

  afterEach(() => {
    vi.doUnmock('~/model/api')
    vi.restoreAllMocks()
  })

  // A Watch stream that yields `events`, then ends, fails, or stays open until `signal` aborts.
  async function* stream(
    signal: AbortSignal,
    events: WatchEntitiesResponse[],
    then: 'end' | 'fail' | 'hold',
  ): AsyncGenerator<WatchEntitiesResponse> {
    for (const event of events) yield event
    if (then === 'fail') throw new Error('connection refused')
    if (then === 'hold') {
      await new Promise<never>((_, reject) => signal.addEventListener('abort', () => reject(new Error('aborted'))))
    }
  }

  const connection = (): ConnectionState => useReplica.getState().connection

  // Every change of the connection state from now on, in order.
  function recordConnection(): ConnectionState[] {
    const seen: ConnectionState[] = []
    let last = connection()
    useReplica.subscribe((s) => {
      if (s.connection === last) return
      last = s.connection
      seen.push(last)
    })
    return seen
  }

  it('is retrying after a failed attempt and live after the next snapshot', async () => {
    watchEntities
      .mockImplementationOnce((_, { signal }) => stream(signal, [], 'fail'))
      .mockImplementationOnce((_, { signal }) => stream(signal, [watch.snapshot(entity('a'))], 'hold'))
    const seen = recordConnection()
    const controller = new AbortController()
    const done = sync.runEntitySync(controller.signal, 0)
    expect(connection()).toBe('connecting')
    await vi.waitFor(() => expect(connection()).toBe('live'))
    expect(seen).toEqual(['retrying', 'live'])
    expect(watchEntities).toHaveBeenCalledTimes(2)
    controller.abort()
    await done
  })

  it('is retrying when a stream ends', async () => {
    watchEntities
      .mockImplementationOnce((_, { signal }) => stream(signal, [watch.snapshot(entity('a'))], 'end'))
      .mockImplementation((_, { signal }) => stream(signal, [], 'hold'))
    const seen = recordConnection()
    const controller = new AbortController()
    const done = sync.runEntitySync(controller.signal, 0)
    await vi.waitFor(() => expect(watchEntities).toHaveBeenCalledTimes(2))
    expect(seen).toEqual(['live', 'retrying'])
    expect(connection()).toBe('retrying')
    controller.abort()
    await done
  })

  it('stops on abort without setting retrying', async () => {
    watchEntities.mockImplementation((_, { signal }) => stream(signal, [watch.snapshot(entity('a'))], 'hold'))
    const seen = recordConnection()
    const controller = new AbortController()
    const done = sync.runEntitySync(controller.signal, 0)
    await vi.waitFor(() => expect(connection()).toBe('live'))
    controller.abort()
    await done
    expect(seen).toEqual(['live'])
    expect(connection()).toBe('live')
    expect(watchEntities).toHaveBeenCalledTimes(1)
  })
})
