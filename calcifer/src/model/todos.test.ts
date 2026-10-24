import { beforeEach, describe, expect, it, vi } from 'vitest'
import { create } from '@bufbuild/protobuf'
import {
  EntityRefListSchema,
  EntitySchema,
  PropertyValueSchema,
  type Entity,
  type PropertyValue,
} from '@calcifer/proto/calcifer/v1/entities_pb'
import { ListStructuresResponseSchema } from '@calcifer/proto/calcifer/v1/services_pb'
import { PropertyKind } from '@calcifer/proto/calcifer/v1/structures_pb'
import { queryClient } from '~/model/api'
import { structuresQuery } from '~/model/structures'
import { entity } from '~/model/testing'
import { DEFAULT_TODO_SORT, parseTodoInput, selectTodos, todoFields, type TodoFilter } from '~/model/todos'

vi.mock('~/model/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('~/model/api')>()),
  entityClient: {},
  richTextClient: {},
  structureClient: {},
}))

const TODAY = '2026-10-23'
const ALL: TodoFilter = { status: 'all' }

// Priority options deliberately not in alphabetical order, so a sort by key would differ.
beforeEach(() => {
  queryClient.clear()
  queryClient.setQueryData(
    structuresQuery.queryKey,
    create(ListStructuresResponseSchema, {
      facets: [
        {
          id: 'todo',
          properties: [
            {
              id: 'todo.status',
              kind: PropertyKind.SELECT,
              options: [
                { key: 'open', label: 'Open' },
                { key: 'done', label: 'Done' },
              ],
              defaultOption: 'open',
            },
            {
              id: 'todo.priority',
              kind: PropertyKind.SELECT,
              options: [
                { key: 'none', label: 'None' },
                { key: 'low', label: 'Low' },
                { key: 'medium', label: 'Medium' },
                { key: 'high', label: 'High' },
              ],
              defaultOption: 'none',
            },
            { id: 'todo.due', kind: PropertyKind.DATE },
          ],
        },
      ],
    }),
  )
})

interface TodoInit {
  status?: string
  priority?: string
  due?: string
  tags?: string[]
}

function todo(id: string, { status, priority, due, tags }: TodoInit = {}): Entity {
  const properties: Record<string, PropertyValue> = {}
  const set = (key: string, value: PropertyValue['value']) => {
    properties[key] = create(PropertyValueSchema, { value })
  }
  if (status !== undefined) set('todo.status', { case: 'select', value: status })
  if (priority !== undefined) set('todo.priority', { case: 'select', value: priority })
  if (due !== undefined) set('todo.due', { case: 'date', value: due })
  if (tags !== undefined) {
    const refs = tags.map((id) => ({ id, structureType: 'Tag' }))
    set('tags', { case: 'relations', value: create(EntityRefListSchema, { refs }) })
  }
  return create(EntitySchema, { ...entity(id), facets: ['todo'], properties })
}

const ids = (entities: Entity[]) => entities.map((e) => e.id)

describe('selectTodos', () => {
  it('keeps only entities with the todo facet', () => {
    const plain = create(EntitySchema, {
      ...entity('plain'),
      properties: { 'todo.status': create(PropertyValueSchema, { value: { case: 'select', value: 'open' } }) },
    })
    expect(ids(selectTodos([todo('a'), plain, todo('b')], ALL, DEFAULT_TODO_SORT, TODAY))).toEqual(['a', 'b'])
  })

  it('filters on todo.status', () => {
    const entities = [todo('open', { status: 'open' }), todo('done', { status: 'done' }), todo('unset')]
    expect(ids(selectTodos(entities, { status: 'done' }, DEFAULT_TODO_SORT, TODAY))).toEqual(['done'])
    expect(ids(selectTodos(entities, { status: 'open' }, DEFAULT_TODO_SORT, TODAY))).toEqual(['open', 'unset'])
  })

  it('filters on todo.priority', () => {
    const entities = [todo('high', { priority: 'high' }), todo('low', { priority: 'low' })]
    expect(ids(selectTodos(entities, { ...ALL, priority: 'high' }, DEFAULT_TODO_SORT, TODAY))).toEqual(['high'])
  })

  it('filters on todo.due', () => {
    const entities = [
      todo('past', { due: '2026-10-01' }),
      todo('today', { due: TODAY }),
      todo('soon', { due: '2026-10-27' }),
      todo('later', { due: '2026-12-01' }),
      todo('none'),
    ]
    const by = (due: TodoFilter['due']) => ids(selectTodos(entities, { ...ALL, due }, DEFAULT_TODO_SORT, TODAY))
    expect(by('overdue')).toEqual(['past'])
    expect(by('today')).toEqual(['today'])
    expect(by('week')).toEqual(['today', 'soon'])
    expect(by('none')).toEqual(['none'])
  })

  it('filters on the tags property', () => {
    const entities = [todo('a', { tags: ['t1', 't2'] }), todo('b', { tags: ['t2'] }), todo('c')]
    expect(ids(selectTodos(entities, { ...ALL, tag: 't1' }, DEFAULT_TODO_SORT, TODAY))).toEqual(['a'])
  })

  it('sorts by priority in registry option order', () => {
    const entities = [
      todo('medium', { priority: 'medium' }),
      todo('high', { priority: 'high' }),
      todo('none'),
      todo('low', { priority: 'low' }),
    ]
    const sorted = selectTodos(entities, ALL, { by: 'priority', dir: 'asc' }, TODAY)
    expect(ids(sorted)).toEqual(['none', 'low', 'medium', 'high'])
  })
})

describe('todoFields', () => {
  it('reads a select value that is not an option as the default', () => {
    const fields = todoFields(todo('a', { status: 'archived', priority: 'urgent' }))
    expect(fields.status).toBe('open')
    expect(fields.priority).toBe('none')
  })

  it('reads the todo.* values and tags', () => {
    const fields = todoFields(todo('a', { status: 'done', priority: 'low', due: TODAY, tags: ['t1'] }))
    expect(fields).toEqual({ status: 'done', priority: 'low', due: TODAY, tagIds: ['t1'] })
  })
})

describe('parseTodoInput', () => {
  it('moves hashtags into tags and strips them from the name', () => {
    expect(parseTodoInput('Buy milk #errands')).toEqual({ name: 'Buy milk', tags: ['errands'] })
  })

  it('collapses the spaces a token in the middle leaves', () => {
    expect(parseTodoInput('Call  #home mum  #urgent')).toEqual({ name: 'Call mum', tags: ['home', 'urgent'] })
  })

  it('keeps each tag once, in first-seen order', () => {
    expect(parseTodoInput('#b x #a #b')).toEqual({ name: 'x', tags: ['b', 'a'] })
  })

  it('leaves a # inside a word and a bare # as text', () => {
    expect(parseTodoInput('Learn C# # now')).toEqual({ name: 'Learn C# # now', tags: [] })
  })

  it('takes letters, digits and single dots between them', () => {
    expect(parseTodoInput('Ship #v1.2 #Café #2026')).toEqual({ name: 'Ship', tags: ['v1.2', 'Café', '2026'] })
  })

  it('leaves a token with any other character in the name', () => {
    expect(parseTodoInput('a #foo-bar #foo_bar #errands, #v1. #a..b')).toEqual({
      name: 'a #foo-bar #foo_bar #errands, #v1. #a..b',
      tags: [],
    })
  })

  it('returns an empty name when the text is only tags', () => {
    expect(parseTodoInput('#errands')).toEqual({ name: '', tags: ['errands'] })
  })
})
