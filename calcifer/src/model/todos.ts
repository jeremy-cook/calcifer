import { useCallback, useMemo } from 'react'
import type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'
import type { Timestamp } from '@bufbuild/protobuf/wkt'
import { useAllEntities, useUpdateEntity, withProperty } from '~/model/store'
import { shiftIso, todayIso } from '~/model/dates'
import { propertyDef } from '~/model/structures'

const STATUS_DEF = propertyDef('Todo', 'status')
const PRIORITY_DEF = propertyDef('Todo', 'priority')

export const TODO_STATUS_OPTIONS = STATUS_DEF.options
export type TodoStatus = (typeof TODO_STATUS_OPTIONS)[number]['key']

// Rank order low -> high.
export const TODO_PRIORITY_OPTIONS = PRIORITY_DEF.options
export type TodoPriority = (typeof TODO_PRIORITY_OPTIONS)[number]['key']

export interface TodoFields {
  status: TodoStatus
  priority: TodoPriority
  due?: string
  tagIds: string[]
}

export function isOptionKey<K extends string>(options: readonly { key: K }[], v: unknown): v is K {
  return options.some((o) => o.key === v)
}

export function todoFields(entity: Entity): TodoFields {
  const valueOf = (id: string) => entity.properties.find((p) => p.id === id)?.value?.value

  const statusVal = valueOf('status')
  const status: TodoStatus =
    statusVal?.case === 'select' && isOptionKey(TODO_STATUS_OPTIONS, statusVal.value)
      ? statusVal.value
      : STATUS_DEF.default

  const priorityVal = valueOf('priority')
  const priority: TodoPriority =
    priorityVal?.case === 'select' && isOptionKey(TODO_PRIORITY_OPTIONS, priorityVal.value)
      ? priorityVal.value
      : PRIORITY_DEF.default

  const dueVal = valueOf('due')
  const due = dueVal?.case === 'date' ? dueVal.value : undefined

  const tagsVal = valueOf('tags')
  const tagIds = tagsVal?.case === 'relations' ? tagsVal.value.refs.map((r) => r.id) : []

  return { status, priority, due, tagIds }
}

// Filter/sort choices, defined once: route search validation, the toolbar and
// the TodoFilter/TodoSort types all derive from these.
export const TODO_STATUS_FILTER_OPTIONS = [...TODO_STATUS_OPTIONS, { key: 'all', label: 'All' }] as const

// An unset due filter means "any due date".
export const TODO_DUE_FILTER_OPTIONS = [
  { key: 'overdue', label: 'Overdue' },
  { key: 'today', label: 'Today' },
  { key: 'week', label: 'This week' },
  { key: 'none', label: 'No due date' },
] as const

export const TODO_SORT_OPTIONS = [
  { key: 'due', label: 'Due date' },
  { key: 'priority', label: 'Priority' },
  { key: 'created', label: 'Created' },
  { key: 'updated', label: 'Updated' },
  { key: 'name', label: 'Name' },
] as const

export const TODO_SORT_DIR_OPTIONS = [
  { key: 'asc', label: 'Ascending' },
  { key: 'desc', label: 'Descending' },
] as const

export interface TodoFilter {
  status: (typeof TODO_STATUS_FILTER_OPTIONS)[number]['key']
  priority?: TodoPriority
  tag?: string
  due?: (typeof TODO_DUE_FILTER_OPTIONS)[number]['key']
}

export interface TodoSort {
  by: (typeof TODO_SORT_OPTIONS)[number]['key']
  dir: (typeof TODO_SORT_DIR_OPTIONS)[number]['key']
}

export const DEFAULT_TODO_FILTER: TodoFilter = { status: 'open' }
export const DEFAULT_TODO_SORT: TodoSort = { by: 'due', dir: 'asc' }

// Route-search shape shared by /s/Todo's validateSearch and TodoToolbar's onChange.
export interface TodoSearch {
  status?: TodoFilter['status']
  priority?: TodoPriority
  tag?: string
  due?: TodoFilter['due']
  sort?: TodoSort['by']
  dir?: TodoSort['dir']
}

function timestampMillis(ts: Timestamp | undefined): number {
  if (!ts) return 0
  return Number(ts.seconds) * 1000 + ts.nanos / 1_000_000
}

function matchesDue(due: string | undefined, filter: NonNullable<TodoFilter['due']>, todayIsoValue: string): boolean {
  if (filter === 'none') return due === undefined
  if (due === undefined) return false
  if (filter === 'overdue') return due < todayIsoValue
  if (filter === 'today') return due === todayIsoValue
  // week: today through 6 days out, inclusive.
  return due >= todayIsoValue && due <= shiftIso(todayIsoValue, 6)
}

function priorityRank(priority: TodoPriority): number {
  return TODO_PRIORITY_OPTIONS.findIndex((o) => o.key === priority)
}

function compareTodos(a: Entity, b: Entity, sort: TodoSort): number {
  const af = todoFields(a)
  const bf = todoFields(b)
  let cmp = 0
  switch (sort.by) {
    case 'due': {
      if (af.due === undefined && bf.due === undefined) cmp = 0
      else if (af.due === undefined) cmp = 1
      else if (bf.due === undefined) cmp = -1
      else cmp = af.due.localeCompare(bf.due)
      break
    }
    case 'priority':
      cmp = priorityRank(af.priority) - priorityRank(bf.priority)
      break
    case 'created':
      cmp = timestampMillis(a.createdAt) - timestampMillis(b.createdAt)
      break
    case 'updated':
      cmp = timestampMillis(a.updatedAt) - timestampMillis(b.updatedAt)
      break
    case 'name':
      cmp = a.name.localeCompare(b.name)
      break
  }
  return sort.dir === 'desc' ? -cmp : cmp
}

export function selectTodos(entities: Entity[], filter: TodoFilter, sort: TodoSort, todayIsoValue: string): Entity[] {
  const matches = entities.filter((e) => {
    if (e.structureType !== 'Todo') return false
    const fields = todoFields(e)
    if (filter.status !== 'all' && fields.status !== filter.status) return false
    if (filter.priority !== undefined && fields.priority !== filter.priority) return false
    if (filter.tag !== undefined && !fields.tagIds.includes(filter.tag)) return false
    if (filter.due !== undefined && !matchesDue(fields.due, filter.due, todayIsoValue)) return false
    return true
  })
  return matches.sort((a, b) => compareTodos(a, b, sort))
}

export function todoDue(entity: Entity): string | undefined {
  if (entity.structureType !== 'Todo') return undefined
  return todoFields(entity).due
}

export function useSetTodoStatus() {
  const updateEntity = useUpdateEntity()
  return useCallback(
    (entity: Entity, status: TodoStatus) =>
      updateEntity(withProperty(entity, 'status', { case: 'select', value: status })),
    [updateEntity],
  )
}

export function useTodos(filter: TodoFilter, sort: TodoSort): Entity[] {
  const entities = useAllEntities()
  return useMemo(() => selectTodos(entities, filter, sort, todayIso()), [entities, filter, sort])
}
