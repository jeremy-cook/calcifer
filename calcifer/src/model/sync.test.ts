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
    sync.applyWatchEvent(watch.deletedId('b'))
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
