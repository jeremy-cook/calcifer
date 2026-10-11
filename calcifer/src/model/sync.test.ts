import { beforeEach, describe, expect, it, vi } from 'vitest'
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
  it('replaces the set with a snapshot, dropping entities it omits', async () => {
    await sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    await sync.applyWatchEvent(watch.snapshot(entity('b'), entity('c')))
    expect(ids()).toEqual(['b', 'c'])
  })

  it('appends an upserted new id', async () => {
    await sync.applyWatchEvent(watch.snapshot(entity('a')))
    await sync.applyWatchEvent(watch.upserted(entity('b')))
    expect(ids()).toEqual(['a', 'b'])
  })

  it('replaces an upserted existing id in place', async () => {
    await sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b'), entity('c')))
    await sync.applyWatchEvent(watch.upserted(entity('b', 'renamed', 2_000)))
    expect(ids()).toEqual(['a', 'b', 'c'])
    expect(store.getEntitiesSnapshot()[1].name).toBe('renamed')
  })

  it('removes a deleted id', async () => {
    await sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    await sync.applyWatchEvent(watch.deletedId('a'))
    expect(ids()).toEqual(['b'])
  })

  it('lets a later snapshot win over earlier events', async () => {
    await sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    await sync.applyWatchEvent(watch.upserted(entity('c')))
    await sync.applyWatchEvent(watch.upserted(entity('a', 'renamed', 2_000)))
    await sync.applyWatchEvent(watch.deletedId('b'))
    await sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    expect(store.getEntitiesSnapshot()).toEqual([entity('a'), entity('b')])
  })

  it('holds the snapshot before it invalidates rich text', async () => {
    const seen: string[][] = []
    vi.spyOn(api.queryClient, 'invalidateQueries').mockImplementation(async () => {
      seen.push(ids())
    })
    await sync.applyWatchEvent(watch.snapshot(entity('a')))
    await sync.applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    expect(seen).toEqual([['a'], ['a', 'b']])
  })

  it('writes a changed rich-text doc to its cache', async () => {
    await sync.applyWatchEvent(watch.richTextChanged(richText('a', 'content', '{"v":1}', 2_000)))
    expect(api.queryClient.getQueryData(api.qk.richtext('a', 'content'))).toMatchObject({ doc: '{"v":1}' })
  })
})
