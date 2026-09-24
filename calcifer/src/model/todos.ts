import { useCallback, useMemo } from 'react'
import type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'
import type { Timestamp } from '@bufbuild/protobuf/wkt'
import { useAllEntities, useUpdateEntity, withProperty } from '~/model/store'
import { shiftIso, useToday } from '~/model/dates'
import { getStructure, propertyDef, type StructureDef } from '~/model/structures'

// Option keys, labels and defaults come from the fetched registry. Keys the UI
// branches on (status 'done'/'open', priority 'none') are behaviour, not schema,
// and stay literal where they're used.
export type TodoStatus = string
export type TodoPriority = string

export interface OptionItem {
  key: string
  label: string
}

function todoStatusOptions(todo: StructureDef | undefined): readonly OptionItem[] {
  return propertyDef(todo, 'status')?.options ?? []
}

// In rank order, low -> high.
export function todoPriorityOptions(todo: StructureDef | undefined): readonly OptionItem[] {
  return propertyDef(todo, 'priority')?.options ?? []
}

export interface TodoFields {
  status: TodoStatus
  priority: TodoPriority
  due?: string
  tagIds: string[]
}

export function isOptionKey<K extends string>(options: readonly { key: K }[], v: unknown): v is K {
  return options.some((o) => o.key === v)
}

// A select value that isn't a declared option reads as the declared default.
function selectValue(entity: Entity, todo: StructureDef | undefined, id: string): string {
  const def = propertyDef(todo, id)
  const value = entity.properties.find((p) => p.id === id)?.value?.value
  return value?.case === 'select' && def && isOptionKey(def.options, value.value)
    ? value.value
    : (def?.defaultOption ?? '')
}

export function todoFields(entity: Entity): TodoFields {
  const valueOf = (id: string) => entity.properties.find((p) => p.id === id)?.value?.value
  const todo = getStructure('Todo')

  const status = selectValue(entity, todo, 'status')
  const priority = selectValue(entity, todo, 'priority')

  const dueVal = valueOf('due')
  const due = dueVal?.case === 'date' ? dueVal.value : undefined

  const tagsVal = valueOf('tags')
  const tagIds = tagsVal?.case === 'relations' ? tagsVal.value.refs.map((r) => r.id) : []

  return { status, priority, due, tagIds }
}

// Filter/sort choices, defined once: route search validation, the toolbar and
// the TodoFilter/TodoSort types all derive from these.
export function todoStatusFilterOptions(todo: StructureDef | undefined): readonly OptionItem[] {
  return [...todoStatusOptions(todo), { key: 'all', label: 'All' }]
}

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
  // A status option key, or 'all'.
  status: string
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

// `priorityOrder` is the priority option keys in registry order, which is rank order.
function compareTodos(a: Entity, b: Entity, sort: TodoSort, priorityOrder: readonly string[]): number {
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
      cmp = priorityOrder.indexOf(af.priority) - priorityOrder.indexOf(bf.priority)
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
  const priorityOrder = todoPriorityOptions(getStructure('Todo')).map((o) => o.key)
  return matches.sort((a, b) => compareTodos(a, b, sort, priorityOrder))
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
  const today = useToday()
  return useMemo(() => selectTodos(entities, filter, sort, today), [entities, filter, sort, today])
}
