import { useCallback, useMemo } from 'react'
import type { Entity } from '@calcifer/proto/calcifer/v1/entities_pb'
import { hasFacet, propertyValueOf, useEntitiesWithFacet, usePropertyWriters } from '~/model/store'
import { shiftIso, timestampMsOrZero, useToday } from '~/model/dates'
import { getFacet, propertyDef, type FacetDef } from '~/model/structures'

// A to-do is any entity with the `todo` facet; its values live under the facet's
// namespaced ids. Option keys, labels and defaults come from the fetched registry. Keys the UI
// branches on (status 'done'/'open', priority 'none') are behaviour, not schema,
// and stay literal where they're used.
export type TodoStatus = string
export type TodoPriority = string

export interface OptionItem {
  key: string
  label: string
}

export const TODO_FACET = 'todo'
export const TODO_STATUS = 'todo.status'
export const TODO_PRIORITY = 'todo.priority'
export const TODO_DUE = 'todo.due'

function todoStatusOptions(todo: FacetDef | undefined): readonly OptionItem[] {
  return propertyDef(todo, TODO_STATUS)?.options ?? []
}

// In rank order, low -> high.
export function todoPriorityOptions(todo: FacetDef | undefined): readonly OptionItem[] {
  return propertyDef(todo, TODO_PRIORITY)?.options ?? []
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
function selectValue(entity: Entity, todo: FacetDef | undefined, id: string): string {
  const def = propertyDef(todo, id)
  const value = propertyValueOf(entity, id)
  return value?.case === 'select' && def && isOptionKey(def.options, value.value)
    ? value.value
    : (def?.defaultOption ?? '')
}

export function todoFields(entity: Entity): TodoFields {
  const valueOf = (id: string) => propertyValueOf(entity, id)
  const todo = getFacet(TODO_FACET)

  const status = selectValue(entity, todo, TODO_STATUS)
  const priority = selectValue(entity, todo, TODO_PRIORITY)

  const dueVal = valueOf(TODO_DUE)
  const due = dueVal?.case === 'date' ? dueVal.value : undefined

  // Tags are a structure property, not part of the facet.
  const tagsVal = valueOf('tags')
  const tagIds = tagsVal?.case === 'relations' ? tagsVal.value.refs.map((r) => r.id) : []

  return { status, priority, due, tagIds }
}

// Filter/sort choices, defined once: route search validation, the toolbar and
// the TodoFilter/TodoSort types all derive from these.
export function todoStatusFilterOptions(todo: FacetDef | undefined): readonly OptionItem[] {
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
      cmp = timestampMsOrZero(a.createdAt) - timestampMsOrZero(b.createdAt)
      break
    case 'updated':
      cmp = timestampMsOrZero(a.updatedAt) - timestampMsOrZero(b.updatedAt)
      break
    case 'name':
      cmp = a.name.localeCompare(b.name)
      break
  }
  return sort.dir === 'desc' ? -cmp : cmp
}

export function selectTodos(entities: Entity[], filter: TodoFilter, sort: TodoSort, todayIsoValue: string): Entity[] {
  const matches = entities.filter((e) => {
    if (!hasFacet(e, TODO_FACET)) return false
    const fields = todoFields(e)
    if (filter.status !== 'all' && fields.status !== filter.status) return false
    if (filter.priority !== undefined && fields.priority !== filter.priority) return false
    if (filter.tag !== undefined && !fields.tagIds.includes(filter.tag)) return false
    if (filter.due !== undefined && !matchesDue(fields.due, filter.due, todayIsoValue)) return false
    return true
  })
  const priorityOrder = todoPriorityOptions(getFacet(TODO_FACET)).map((o) => o.key)
  return matches.sort((a, b) => compareTodos(a, b, sort, priorityOrder))
}

export function todoDue(entity: Entity): string | undefined {
  if (!hasFacet(entity, TODO_FACET)) return undefined
  return todoFields(entity).due
}

export function useSetTodoStatus() {
  const { setSelect } = usePropertyWriters()
  return useCallback((entity: Entity, status: TodoStatus) => setSelect(entity, TODO_STATUS, status), [setSelect])
}

export function useTodos(filter: TodoFilter, sort: TodoSort): Entity[] {
  const entities = useEntitiesWithFacet(TODO_FACET)
  const today = useToday()
  return useMemo(() => selectTodos(entities, filter, sort, today), [entities, filter, sort, today])
}
