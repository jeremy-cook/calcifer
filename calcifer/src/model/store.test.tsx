// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { create } from '@bufbuild/protobuf'
import { EntitySchema, PropertyValueSchema, type Entity } from '@calcifer/proto/calcifer/v1/entities_pb'
import { entityClient, queryClient } from '~/model/api'
import { replica, resetTombstones, useReplica } from '~/model/replica'
import {
  getEntity,
  propertyValueOf,
  useAllEntities,
  useEntitiesByIds,
  useEntitiesOfType,
  useEntity,
  useRenameEntity,
  useSetProperty,
} from '~/model/store'
import { applyWatchEvent } from '~/model/sync'
import { entity, watch } from '~/model/testing'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: { renameEntity: vi.fn(), setEntityProperty: vi.fn() },
  richTextClient: {},
  structureClient: {},
}))

beforeEach(() => {
  useReplica.setState(useReplica.getInitialState(), true)
  resetTombstones()
  queryClient.clear()
})

afterEach(() => {
  vi.clearAllMocks()
})

interface WrapperProps {
  children: React.ReactNode
}

function wrapper({ children }: WrapperProps) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

function withText(e: Entity, propertyId: string, text: string): Entity {
  const value = create(PropertyValueSchema, { value: { case: 'text', value: text } })
  return create(EntitySchema, { ...e, properties: { ...e.properties, [propertyId]: value } })
}

// The text property's value; `null` when the entity doesn't have it.
function textOf(id: string, propertyId: string): string | null {
  const value = propertyValueOf(getEntity(id)!, propertyId)
  return value?.case === 'text' ? value.value : null
}

// A promise the test settles by hand.
function deferred<T>() {
  let reject!: (err: Error) => void
  const promise = new Promise<T>((_resolve, rej) => {
    reject = rej
  })
  return { promise, reject }
}

describe('useEntity', () => {
  it('is loading before the first snapshot', () => {
    const { result } = renderHook(() => useEntity('a'))
    expect(result.current).toEqual({ entity: undefined, status: 'loading' })
  })

  it('is found after a snapshot that holds the id', () => {
    const { result } = renderHook(() => useEntity('a'))
    act(() => applyWatchEvent(watch.snapshot(entity('a'))))
    expect(result.current.status).toBe('found')
    expect(result.current.entity?.id).toBe('a')
  })

  it("is missing after a snapshot that doesn't hold the id", () => {
    const { result } = renderHook(() => useEntity('a'))
    act(() => applyWatchEvent(watch.snapshot(entity('b'))))
    expect(result.current).toEqual({ entity: undefined, status: 'missing' })
  })

  it('is missing right after the id is deleted', () => {
    applyWatchEvent(watch.snapshot(entity('a')))
    const { result } = renderHook(() => useEntity('a'))
    act(() => applyWatchEvent(watch.deletedId('a')))
    expect(result.current).toEqual({ entity: undefined, status: 'missing' })
  })
})

describe('useAllEntities', () => {
  it("doesn't re-render on an echo", () => {
    applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    let renders = 0
    renderHook(() => {
      renders++
      return useAllEntities()
    })
    act(() => applyWatchEvent(watch.upserted(entity('a'))))
    act(() => applyWatchEvent(watch.snapshot(entity('a'), entity('b'))))
    expect(renders).toBe(1)
  })
})

describe('useEntitiesOfType', () => {
  it('re-renders only when an entity of that type changes', () => {
    const tag = (id: string, name = id, updatedAtMs = 1_000) =>
      create(EntitySchema, { ...entity(id, name, updatedAtMs), structureType: 'Tag' })
    applyWatchEvent(watch.snapshot(tag('t1'), entity('a')))
    let renders = 0
    const { result } = renderHook(() => {
      renders++
      return useEntitiesOfType('Tag')
    })
    act(() => applyWatchEvent(watch.upserted(entity('a', 'edited', 2_000))))
    expect(renders).toBe(1)
    act(() => applyWatchEvent(watch.upserted(tag('t2'))))
    expect(renders).toBe(2)
    expect(result.current.map((e) => e.id)).toEqual(['t1', 't2'])
  })
})

describe('useEntitiesByIds', () => {
  it('re-renders only when one of its ids changes', () => {
    applyWatchEvent(watch.snapshot(entity('a'), entity('b')))
    let renders = 0
    const { result } = renderHook(() => {
      renders++
      return useEntitiesByIds(['a', 'missing'])
    })
    expect(result.current.map((e) => e?.id)).toEqual(['a', undefined])
    act(() => applyWatchEvent(watch.upserted(entity('b', 'edited', 2_000))))
    expect(renders).toBe(1)
    act(() => applyWatchEvent(watch.upserted(entity('a', 'edited', 2_000))))
    expect(renders).toBe(2)
    expect(result.current[0]?.name).toBe('edited')
  })
})

describe('useRenameEntity', () => {
  it('restores only the name on error, keeping a concurrent property edit (I-6)', async () => {
    applyWatchEvent(watch.snapshot(entity('a', 'Old')))
    const rename = deferred<never>()
    vi.mocked(entityClient.renameEntity).mockReturnValue(rename.promise)
    const { result } = renderHook(() => useRenameEntity(), { wrapper })

    act(() => result.current('a', 'New'))
    await vi.waitFor(() => expect(getEntity('a')?.name).toBe('New'))
    act(() => replica.patch('a', (e) => withText(e, 'note', 'concurrent')))
    await act(async () => rename.reject(new Error('rename failed')))

    await vi.waitFor(() => expect(getEntity('a')?.name).toBe('Old'))
    expect(textOf('a', 'note')).toBe('concurrent')
  })
})

describe('useSetProperty', () => {
  it('restores only that property on error and calls onError', async () => {
    applyWatchEvent(watch.snapshot(withText(entity('a'), 'title', 'before')))
    const set = deferred<never>()
    vi.mocked(entityClient.setEntityProperty).mockReturnValue(set.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useSetProperty({ onError }), { wrapper })

    act(() => result.current(getEntity('a')!, 'title', { case: 'text', value: 'after' }))
    await vi.waitFor(() => expect(textOf('a', 'title')).toBe('after'))
    act(() => replica.patch('a', (e) => withText(e, 'note', 'concurrent')))
    await act(async () => set.reject(new Error('set failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(textOf('a', 'title')).toBe('before')
    expect(textOf('a', 'note')).toBe('concurrent')
  })

  it('removes a property that was absent before a failed write', async () => {
    applyWatchEvent(watch.snapshot(entity('a')))
    const set = deferred<never>()
    vi.mocked(entityClient.setEntityProperty).mockReturnValue(set.promise)
    const { result } = renderHook(() => useSetProperty(), { wrapper })

    act(() => result.current(getEntity('a')!, 'title', { case: 'text', value: 'after' }))
    await vi.waitFor(() => expect(textOf('a', 'title')).toBe('after'))
    await act(async () => set.reject(new Error('set failed')))

    await vi.waitFor(() => expect(textOf('a', 'title')).toBeNull())
  })
})
