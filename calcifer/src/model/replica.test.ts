import { beforeEach, describe, expect, it } from 'vitest'
import { create } from '@bufbuild/protobuf'
import { EntitySchema } from '@calcifer/proto/calcifer/v1/entities_pb'
import { replica, useReplica } from '~/model/replica'
import { entity } from '~/model/testing'

beforeEach(() => {
  replica.reset()
})

const state = () => useReplica.getState()

describe('replica', () => {
  it('starts connecting and resets to connecting', () => {
    expect(state().connection).toBe('connecting')
    replica.setConnection('live')
    replica.reset()
    expect(state().connection).toBe('connecting')
  })

  it('setConnection changes only the connection', () => {
    replica.applySnapshot([entity('a')])
    const { byId, all, ready } = state()
    replica.setConnection('retrying')
    expect(state().connection).toBe('retrying')
    expect(state().byId).toBe(byId)
    expect(state().all).toBe(all)
    expect(state().ready).toBe(ready)
  })

  it('setConnection to the same state changes nothing', () => {
    replica.setConnection('live')
    const before = state()
    replica.setConnection('live')
    expect(state()).toBe(before)
  })

  it('keeps the connection across entity writes', () => {
    replica.setConnection('live')
    replica.applySnapshot([entity('a')])
    replica.upsert(entity('b'))
    replica.remove('a')
    expect(state().connection).toBe('live')
  })

  it('keeps every reference on an echo', () => {
    replica.applySnapshot([entity('a'), entity('b')])
    const before = state()
    replica.upsert(entity('a'))
    expect(state()).toBe(before)
    expect(state().all).toBe(before.all)
  })

  it('keeps an unchanged entity across a resync snapshot', () => {
    replica.applySnapshot([entity('a'), entity('b')])
    const a = state().byId.get('a')
    replica.applySnapshot([entity('a'), entity('b', 'renamed', 2_000)])
    expect(state().byId.get('a')).toBe(a)
    expect(state().byId.get('b')?.name).toBe('renamed')
  })

  it('changes nothing on an identical resync snapshot', () => {
    replica.applySnapshot([entity('a'), entity('b')])
    const before = state()
    replica.applySnapshot([entity('a'), entity('b')])
    expect(state()).toBe(before)
  })

  it('lets the first snapshot replace an earlier upsert and become ready', () => {
    replica.upsert(entity('early'))
    expect(state().ready).toBe(false)
    replica.applySnapshot([entity('a')])
    expect(state().ready).toBe(true)
    expect(state().all.map((e) => e.id)).toEqual(['a'])
  })

  it('becomes ready on an empty first snapshot', () => {
    replica.applySnapshot([])
    expect(state().ready).toBe(true)
  })

  it('replaces an upserted entity in its slot', () => {
    replica.applySnapshot([entity('a'), entity('b'), entity('c')])
    replica.upsert(entity('b', 'renamed', 2_000))
    expect(state().all.map((e) => e.name)).toEqual(['a', 'renamed', 'c'])
  })

  it('removes an id and ignores one it does not hold', () => {
    replica.applySnapshot([entity('a'), entity('b')])
    replica.remove('a')
    const before = state()
    replica.remove('missing')
    expect(state()).toBe(before)
    expect(state().all.map((e) => e.id)).toEqual(['b'])
  })

  it('patches the cached entity', () => {
    replica.applySnapshot([entity('a')])
    replica.patch('a', (e) => create(EntitySchema, { ...e, name: 'patched' }))
    expect(state().byId.get('a')?.name).toBe('patched')
  })

  it('does nothing when patching an id it does not hold', () => {
    replica.applySnapshot([entity('a')])
    const before = state()
    replica.patch('missing', (e) => create(EntitySchema, { ...e, name: 'patched' }))
    expect(state()).toBe(before)
  })

  it('drops an upsert older than the cached entity', () => {
    replica.applySnapshot([entity('a', 'new', 2_000)])
    replica.upsert(entity('a', 'old', 1_000))
    expect(state().byId.get('a')?.name).toBe('new')
  })

  it('accepts an upsert with an equal timestamp', () => {
    replica.applySnapshot([entity('a', 'first', 2_000)])
    replica.upsert(entity('a', 'second', 2_000))
    expect(state().byId.get('a')?.name).toBe('second')
  })

  it('patches regardless of timestamps', () => {
    replica.applySnapshot([entity('a', 'a', 2_000)])
    replica.patch('a', () => entity('a', 'patched', 1_000))
    expect(state().byId.get('a')?.name).toBe('patched')
  })

  it('drops an upsert for an id removed this session, even after a later snapshot', () => {
    replica.applySnapshot([entity('a'), entity('b')])
    replica.remove('a')
    replica.upsert(entity('a', 'back', 2_000))
    expect(state().byId.has('a')).toBe(false)
    replica.applySnapshot([entity('b')])
    replica.upsert(entity('a', 'back', 3_000))
    expect(state().byId.has('a')).toBe(false)
  })

  it('does not filter a snapshot by timestamps', () => {
    replica.applySnapshot([entity('a', 'new', 2_000)])
    replica.applySnapshot([entity('a', 'old', 1_000)])
    expect(state().byId.get('a')?.name).toBe('old')
  })

  it('drops an id removed this session from a later snapshot and keeps the rest', () => {
    replica.applySnapshot([entity('a'), entity('b'), entity('c')])
    replica.remove('b')
    replica.applySnapshot([entity('a'), entity('b'), entity('c')])
    expect(state().byId.has('b')).toBe(false)
    expect(state().all.map((e) => e.id)).toEqual(['a', 'c'])
  })

  it('changes nothing when a resync differs only by an id removed this session', () => {
    replica.applySnapshot([entity('a'), entity('b')])
    replica.remove('b')
    const before = state()
    replica.applySnapshot([entity('a'), entity('b')])
    expect(state()).toBe(before)
  })
})
