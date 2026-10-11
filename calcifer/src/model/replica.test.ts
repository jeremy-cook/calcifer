import { beforeEach, describe, expect, it } from 'vitest'
import { create } from '@bufbuild/protobuf'
import { EntitySchema } from '@calcifer/proto/calcifer/v1/entities_pb'
import { replica, useReplica } from '~/model/replica'
import { entity } from '~/model/testing'

beforeEach(() => {
  useReplica.setState(useReplica.getInitialState(), true)
})

const state = () => useReplica.getState()

describe('replica', () => {
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
})
