// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { create } from '@bufbuild/protobuf'
import { EntitySchema, PropertyValueSchema, type Entity } from 'calcifer/v1/entities_pb'
import { entityClient, queryClient } from '~/model/api'
import { replica } from '~/model/replica'
import {
  getEntity,
  propertyValueOf,
  relationsValue,
  useAddRelationship,
  useAddRelationTarget,
  useAllEntities,
  useEntitiesByIds,
  useEntitiesOfType,
  useEntitiesWithFacet,
  useEntity,
  useRemoveRelationship,
  useRemoveRelationTarget,
  useRenameEntity,
  useSetProperty,
} from '~/model/store'
import { applyWatchEvent } from '~/model/sync'
import { entity, link, watch, withLinks } from '~/model/testing'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: {
    renameEntity: vi.fn(),
    setEntityProperty: vi.fn(),
    addRelationTarget: vi.fn(),
    removeRelationTarget: vi.fn(),
    addRelationship: vi.fn(),
    removeRelationship: vi.fn(),
  },
  richTextClient: {},
  structureClient: {},
}))

beforeEach(() => {
  replica.reset()
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

function withTags(e: Entity, ...ids: string[]): Entity {
  const tags = relationsValue(ids.map((id) => ({ id, structureType: 'Tag' })))
  return create(EntitySchema, { ...e, properties: { ...e.properties, tags } })
}

// The ids in the entity's `tags` property, in order; `null` when it has no such property.
function tagIdsOf(id: string): string[] | null {
  const value = propertyValueOf(getEntity(id)!, 'tags')
  return value?.case === 'relations' ? value.value.refs.map((ref) => ref.id) : null
}

// The entity's links as `<scope>><target id>`, in order.
function linksOf(id: string): string[] {
  return getEntity(id)!.links.map((l) => `${l.sourcePropertyId}>${l.target?.id}`)
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

describe('useEntitiesWithFacet', () => {
  it('re-renders only when an entity with that facet changes', () => {
    const todo = (id: string, name = id, updatedAtMs = 1_000) =>
      create(EntitySchema, { ...entity(id, name, updatedAtMs), facets: ['todo'] })
    applyWatchEvent(watch.snapshot(todo('t1'), entity('a')))
    let renders = 0
    const { result } = renderHook(() => {
      renders++
      return useEntitiesWithFacet('todo')
    })
    act(() => applyWatchEvent(watch.upserted(entity('a', 'edited', 2_000))))
    expect(renders).toBe(1)
    act(() => applyWatchEvent(watch.upserted(todo('t1', 'edited', 2_000))))
    expect(renders).toBe(2)
    act(() => applyWatchEvent(watch.upserted(todo('a', 'attached', 3_000))))
    expect(renders).toBe(3)
    expect(result.current.map((e) => e.id)).toEqual(['t1', 'a'])
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

describe('relation target writers', () => {
  it('adds one target optimistically and sends only that target', async () => {
    applyWatchEvent(watch.snapshot(withTags(entity('a'), 'a-tag')))
    const add = deferred<never>()
    vi.mocked(entityClient.addRelationTarget).mockReturnValue(add.promise)
    const { result } = renderHook(() => useAddRelationTarget(), { wrapper })

    act(() => result.current(getEntity('a')!, 'tags', { id: 'b-tag', structureType: 'Tag' }))

    await vi.waitFor(() => expect(tagIdsOf('a')).toEqual(['a-tag', 'b-tag']))
    expect(entityClient.addRelationTarget).toHaveBeenCalledOnce()
    expect(vi.mocked(entityClient.addRelationTarget).mock.calls[0][0]).toEqual({
      entityId: 'a',
      propertyId: 'tags',
      targetId: 'b-tag',
    })
  })

  it('on a failed add removes only that target, keeping a concurrent add', async () => {
    applyWatchEvent(watch.snapshot(withTags(entity('a'), 'a-tag')))
    const add = deferred<never>()
    vi.mocked(entityClient.addRelationTarget).mockReturnValue(add.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useAddRelationTarget({ onError }), { wrapper })

    act(() => result.current(getEntity('a')!, 'tags', { id: 'b-tag', structureType: 'Tag' }))
    await vi.waitFor(() => expect(tagIdsOf('a')).toEqual(['a-tag', 'b-tag']))
    act(() => replica.patch('a', (e) => withTags(e, 'a-tag', 'b-tag', 'c-tag')))
    await act(async () => add.reject(new Error('add failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(tagIdsOf('a')).toEqual(['a-tag', 'c-tag'])
  })

  it('on a failed remove restores the target', async () => {
    applyWatchEvent(watch.snapshot(withTags(entity('a'), 'a-tag', 'b-tag')))
    const remove = deferred<never>()
    vi.mocked(entityClient.removeRelationTarget).mockReturnValue(remove.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useRemoveRelationTarget({ onError }), { wrapper })

    act(() => result.current(getEntity('a')!, 'tags', 'a-tag'))
    await vi.waitFor(() => expect(tagIdsOf('a')).toEqual(['b-tag']))
    expect(vi.mocked(entityClient.removeRelationTarget).mock.calls[0][0]).toEqual({
      entityId: 'a',
      propertyId: 'tags',
      targetId: 'a-tag',
    })
    await act(async () => remove.reject(new Error('remove failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(tagIdsOf('a')?.toSorted()).toEqual(['a-tag', 'b-tag'])
  })

  it('removing the last target clears the property optimistically', async () => {
    applyWatchEvent(watch.snapshot(withTags(entity('a'), 'a-tag')))
    const remove = deferred<never>()
    vi.mocked(entityClient.removeRelationTarget).mockReturnValue(remove.promise)
    const { result } = renderHook(() => useRemoveRelationTarget(), { wrapper })

    act(() => result.current(getEntity('a')!, 'tags', 'a-tag'))

    await vi.waitFor(() => expect(tagIdsOf('a')).toBeNull())
    expect(Object.hasOwn(getEntity('a')!.properties, 'tags')).toBe(false)
  })

  it('adding a target that is already present changes nothing on error', async () => {
    applyWatchEvent(watch.snapshot(withTags(entity('a'), 'a-tag')))
    const add = deferred<never>()
    vi.mocked(entityClient.addRelationTarget).mockReturnValue(add.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useAddRelationTarget({ onError }), { wrapper })

    const before = getEntity('a')
    act(() => result.current(getEntity('a')!, 'tags', { id: 'a-tag', structureType: 'Tag' }))
    await vi.waitFor(() => expect(entityClient.addRelationTarget).toHaveBeenCalledOnce())
    expect(getEntity('a')).toBe(before)
    await act(async () => add.reject(new Error('add failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(tagIdsOf('a')).toEqual(['a-tag'])
  })
})

describe('relationship writers', () => {
  it('adds a relationship optimistically and rolls back only that link on error', async () => {
    applyWatchEvent(watch.snapshot(withLinks(entity('a'), link('x', 'content'))))
    const add = deferred<never>()
    vi.mocked(entityClient.addRelationship).mockReturnValue(add.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useAddRelationship({ onError }), { wrapper })

    act(() => result.current(getEntity('a')!, { id: 'b', structureType: 'Note' }))

    await vi.waitFor(() => expect(linksOf('a')).toEqual(['content>x', '@related>b']))
    expect(vi.mocked(entityClient.addRelationship).mock.calls[0][0]).toEqual({ entityId: 'a', targetId: 'b' })
    const added = getEntity('a')!.links[1]
    expect(added.target?.structureType).toBe('Note')
    expect(added.createdAt).toBeDefined()

    // Another writer relates `a` to `c` before this one fails.
    act(() => replica.patch('a', (e) => withLinks(e, ...e.links, link('c', '@related'))))
    await act(async () => add.reject(new Error('add failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(linksOf('a')).toEqual(['content>x', '@related>c'])
  })

  it('on a successful add replaces the optimistic link with the server entity', async () => {
    applyWatchEvent(watch.snapshot(entity('a')))
    const server = withLinks(entity('a'), link('b', '@related', 5_000))
    vi.mocked(entityClient.addRelationship).mockResolvedValue({ entity: server } as never)
    const { result } = renderHook(() => useAddRelationship(), { wrapper })

    act(() => result.current(getEntity('a')!, { id: 'b', structureType: 'Note' }))

    await vi.waitFor(() => expect(getEntity('a')).toBe(server))
  })

  it('adding a relationship that is already there changes nothing on error', async () => {
    applyWatchEvent(watch.snapshot(withLinks(entity('a'), link('b', '@related'))))
    const add = deferred<never>()
    vi.mocked(entityClient.addRelationship).mockReturnValue(add.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useAddRelationship({ onError }), { wrapper })

    const before = getEntity('a')
    act(() => result.current(getEntity('a')!, { id: 'b', structureType: 'Note' }))
    await vi.waitFor(() => expect(entityClient.addRelationship).toHaveBeenCalledOnce())
    expect(getEntity('a')).toBe(before)
    await act(async () => add.reject(new Error('add failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(linksOf('a')).toEqual(['@related>b'])
  })

  it('removes a relationship optimistically and restores it on error', async () => {
    const original = link('b', '@related', 5_000)
    applyWatchEvent(
      watch.snapshot(withLinks(entity('a'), original, link('b', 'content', 6_000), link('c', '@related', 7_000))),
    )
    const remove = deferred<never>()
    vi.mocked(entityClient.removeRelationship).mockReturnValue(remove.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useRemoveRelationship({ onError }), { wrapper })

    act(() => result.current(getEntity('a')!, 'b'))

    // The mention of `b` stays: only the relationship goes.
    await vi.waitFor(() => expect(linksOf('a')).toEqual(['content>b', '@related>c']))
    expect(vi.mocked(entityClient.removeRelationship).mock.calls[0][0]).toEqual({ entityId: 'a', targetId: 'b' })
    await act(async () => remove.reject(new Error('remove failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(linksOf('a').toSorted()).toEqual(['@related>b', '@related>c', 'content>b'])
    expect(getEntity('a')!.links).toContain(original)
  })

  it('removing a relationship that is not there changes nothing on error', async () => {
    applyWatchEvent(watch.snapshot(withLinks(entity('a'), link('b', 'content'))))
    const remove = deferred<never>()
    vi.mocked(entityClient.removeRelationship).mockReturnValue(remove.promise)
    const onError = vi.fn()
    const { result } = renderHook(() => useRemoveRelationship({ onError }), { wrapper })

    const before = getEntity('a')
    act(() => result.current(getEntity('a')!, 'b'))
    await vi.waitFor(() => expect(entityClient.removeRelationship).toHaveBeenCalledOnce())
    expect(getEntity('a')).toBe(before)
    await act(async () => remove.reject(new Error('remove failed')))

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce())
    expect(getEntity('a')).toBe(before)
  })
})
