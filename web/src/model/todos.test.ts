import { beforeEach, describe, expect, it, vi } from 'vitest'
import { create } from '@bufbuild/protobuf'
import {
  EntityRefListSchema,
  EntitySchema,
  PropertyValueSchema,
  type Entity,
  type PropertyValue,
} from 'calcifer/v1/entities_pb'
import { ListStructuresResponseSchema } from 'calcifer/v1/services_pb'
import { PropertyKind } from 'calcifer/v1/structures_pb'
import { queryClient } from '~/model/api'
import { structuresQuery } from '~/model/structures'
import { entity } from '~/model/testing'
import {
  DEFAULT_TODO_ORDER,
  parseTodoInput,
  selectTodos,
  todoFields,
  type TodoFilter,
  type TodoOrder,
} from '~/model/todos'

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
    expect(ids(selectTodos([todo('a'), plain, todo('b')], ALL, DEFAULT_TODO_ORDER, TODAY))).toEqual(['a', 'b'])
  })

  it('filters on todo.status', () => {
    const entities = [todo('open', { status: 'open' }), todo('done', { status: 'done' }), todo('unset')]
    expect(ids(selectTodos(entities, { status: 'done' }, DEFAULT_TODO_ORDER, TODAY))).toEqual(['done'])
    expect(ids(selectTodos(entities, { status: 'open' }, DEFAULT_TODO_ORDER, TODAY))).toEqual(['open', 'unset'])
  })

  it('filters on todo.priority', () => {
    const entities = [todo('high', { priority: 'high' }), todo('low', { priority: 'low' })]
    expect(ids(selectTodos(entities, { ...ALL, priority: 'high' }, DEFAULT_TODO_ORDER, TODAY))).toEqual(['high'])
  })

  it('filters on due groups', () => {
    const entities = [
      todo('past', { due: '2026-10-01' }),
      todo('today', { due: TODAY }),
      todo('soon', { due: '2026-10-27' }),
      todo('later', { due: '2026-12-01' }),
      todo('none'),
      todo('done', { status: 'done', due: TODAY }),
    ]
    const by = (due: TodoFilter['due']) => ids(selectTodos(entities, { ...ALL, due }, DEFAULT_TODO_ORDER, TODAY))
    expect(by(['overdue'])).toEqual(['past'])
    expect(by(['today'])).toEqual(['today'])
    expect(by(['week'])).toEqual(['soon'])
    expect(by(['later', 'none'])).toEqual(['later', 'none'])
    expect(by(['done'])).toEqual(['done'])
    expect(by([])).toHaveLength(entities.length)
  })

  it('filters on a month of Later', () => {
    const entities = [
      todo('soon', { due: '2026-10-27' }),
      todo('nov', { due: '2026-11-03' }),
      todo('dec', { due: '2026-12-01' }),
      todo('dec-done', { status: 'done', due: '2026-12-02' }),
    ]
    const by = (due: TodoFilter['due']) => ids(selectTodos(entities, { ...ALL, due }, DEFAULT_TODO_ORDER, TODAY))
    expect(by(['2026-12'])).toEqual(['dec'])
    expect(by(['week', '2026-11'])).toEqual(['soon', 'nov'])
    // A month only picks from Later: this week's dates in it aren't Later.
    expect(by(['2026-10'])).toEqual([])
  })

  it('sorts Later by month before the order within it', () => {
    const entities = [
      todo('dec-high', { priority: 'high', due: '2026-12-20' }),
      todo('nov-low', { priority: 'low', due: '2026-11-29' }),
      todo('nov-high', { priority: 'high', due: '2026-11-30' }),
    ]
    expect(ids(selectTodos(entities, ALL, DEFAULT_TODO_ORDER, TODAY))).toEqual(['nov-high', 'nov-low', 'dec-high'])
  })

  it('filters on the tags property', () => {
    const entities = [todo('a', { tags: ['t1', 't2'] }), todo('b', { tags: ['t2'] }), todo('c')]
    expect(ids(selectTodos(entities, { ...ALL, tag: 't1' }, DEFAULT_TODO_ORDER, TODAY))).toEqual(['a'])
  })

  it('lists groups in page order, whatever the order within them', () => {
    const entities = [
      todo('none', { priority: 'high' }),
      todo('done', { status: 'done' }),
      todo('soon', { due: '2026-10-27' }),
      todo('past', { due: '2026-10-01' }),
    ]
    expect(ids(selectTodos(entities, ALL, DEFAULT_TODO_ORDER, TODAY))).toEqual(['past', 'soon', 'none', 'done'])
  })

  it('orders by priority high first, in registry option order', () => {
    const entities = [
      todo('medium', { priority: 'medium' }),
      todo('high', { priority: 'high' }),
      todo('none'),
      todo('low', { priority: 'low' }),
    ]
    expect(ids(selectTodos(entities, ALL, DEFAULT_TODO_ORDER, TODAY))).toEqual(['high', 'medium', 'low', 'none'])
    const reversed = { ...DEFAULT_TODO_ORDER, reversed: true }
    expect(ids(selectTodos(entities, ALL, reversed, TODAY))).toEqual(['none', 'low', 'medium', 'high'])
  })

  it('breaks ties with the next key, in its own direction', () => {
    const entities = [
      todo('b-late', { priority: 'high', due: '2026-12-09' }),
      todo('a-early', { priority: 'high', due: '2026-12-02' }),
      todo('c-low', { priority: 'low', due: '2026-12-01' }),
    ]
    const order: TodoOrder = { keys: ['priority', 'due', 'name'], reversed: true }
    expect(ids(selectTodos(entities, ALL, order, TODAY))).toEqual(['c-low', 'a-early', 'b-late'])
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
  it('reads the name, then the tags', () => {
    expect(parseTodoInput('Buy milk #errands')).toEqual({ ok: true, name: 'Buy milk', tags: ['errands'] })
  })

  it('takes a line with no tags as all name, with spaces collapsed', () => {
    expect(parseTodoInput('  Learn  C#  ')).toEqual({ ok: true, name: 'Learn C#', tags: [] })
  })

  it('takes letters, digits and single dots in a tag, and keeps each tag once', () => {
    expect(parseTodoInput('Ship #v1.2 #Café #2026 #v1.2')).toEqual({
      ok: true,
      name: 'Ship',
      tags: ['v1.2', 'Café', '2026'],
    })
  })

  it('refuses a line with no name', () => {
    expect(parseTodoInput('#errands')).toEqual({ ok: false, error: 'Add a name before the tags.' })
  })

  it('refuses a word after the tags', () => {
    expect(parseTodoInput('Call #home mum')).toEqual({ ok: false, error: 'Tags go last: "mum" comes after a tag.' })
  })

  it.each(['#foo-bar', '#foo_bar', '#errands,', '#v1.', '#a..b', '#'])('refuses the tag %s', (tag) => {
    expect(parseTodoInput(`Fix it ${tag}`)).toEqual({
      ok: false,
      error: `"${tag}" isn't a valid tag: use letters, digits and dots.`,
    })
  })
})
