import { useCallback, useMemo } from 'react'
import type { Entity } from 'calcifer/v1/entities_pb'
import {
  getOrCreateEntityForMention,
  hasFacet,
  propertyValueOf,
  relationsValue,
  useCreateEntity,
  useEntitiesWithFacet,
  usePropertyWriters,
} from '~/model/store'
import { shiftIso, useToday } from '~/model/dates'
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
// the TodoFilter/TodoOrder types all derive from these.
export function todoStatusFilterOptions(todo: FacetDef | undefined): readonly OptionItem[] {
  return [...todoStatusOptions(todo), { key: 'all', label: 'All' }]
}

// The groups the ToDo page lists under, in page order. Done ToDos all go under Done,
// whatever their date; "This week" is tomorrow through six days out.
export const TODO_GROUPS = [
  { key: 'overdue', label: 'Overdue' },
  { key: 'today', label: 'Today' },
  { key: 'week', label: 'This week' },
  { key: 'later', label: 'Later' },
  { key: 'none', label: 'No date' },
  { key: 'done', label: 'Done' },
] as const

export type TodoGroupKey = (typeof TODO_GROUPS)[number]['key']

export function todoGroup({ status, due }: TodoFields, todayIsoValue: string): TodoGroupKey {
  if (status === 'done') return 'done'
  if (due === undefined) return 'none'
  if (due < todayIsoValue) return 'overdue'
  if (due === todayIsoValue) return 'today'
  if (due <= shiftIso(todayIsoValue, 6)) return 'week'
  return 'later'
}

// A "2026-12" month: the due filter can tick a month of Later on its own.
export type TodoMonthKey = `${number}-${number}`
export type TodoDueKey = TodoGroupKey | TodoMonthKey

const MONTH_KEY = /^\d{4}-\d{2}$/

export function isTodoDueKey(v: unknown): v is TodoDueKey {
  return TODO_GROUPS.some((g) => g.key === v) || (typeof v === 'string' && MONTH_KEY.test(v))
}

// The month a ToDo in Later falls in; undefined in every other group.
export function todoMonth(fields: TodoFields, todayIsoValue: string): TodoMonthKey | undefined {
  if (todoGroup(fields, todayIsoValue) !== 'later') return undefined
  return fields.due?.slice(0, 7) as TodoMonthKey
}

// The order within a group: each key breaks the ties of the one before it.
export const TODO_ORDER_OPTIONS = [
  { key: 'priority', label: 'Priority' },
  { key: 'due', label: 'Due' },
  { key: 'name', label: 'Name' },
] as const

export type TodoOrderKey = (typeof TODO_ORDER_OPTIONS)[number]['key']

export interface TodoFilter {
  // A status option key, or 'all'.
  status: string
  priority?: TodoPriority
  tag?: string
  // Due groups and Later's months to keep; unset or empty is any due date.
  due?: TodoDueKey[]
}

export interface TodoOrder {
  // Every order key, once each, most significant first.
  keys: TodoOrderKey[]
  // Flips the first key: low priority first, latest first, Z–A.
  reversed: boolean
}

export const DEFAULT_TODO_FILTER: TodoFilter = { status: 'open' }
export const DEFAULT_TODO_ORDER: TodoOrder = { keys: ['priority', 'due', 'name'], reversed: false }

// Route-search shape shared by /v/todos' validateSearch and TodoToolbar's onChange.
export interface TodoSearch {
  status?: TodoFilter['status']
  priority?: TodoPriority
  tag?: string
  due?: TodoDueKey[]
  order?: TodoOrderKey[]
  reversed?: boolean
}

// A list of order keys that names each key exactly once.
export function isTodoOrder(v: unknown): v is TodoOrderKey[] {
  return (
    Array.isArray(v) && v.length === TODO_ORDER_OPTIONS.length && TODO_ORDER_OPTIONS.every((o) => v.includes(o.key))
  )
}

function matchesDue(fields: TodoFields, filter: TodoFilter, todayIsoValue: string): boolean {
  const ticked = filter.due ?? []
  if (ticked.length === 0 || ticked.includes(todoGroup(fields, todayIsoValue))) return true
  const month = todoMonth(fields, todayIsoValue)
  return month !== undefined && ticked.includes(month)
}

// Each key in its natural direction: high priority first, soonest first (no date
// last), A–Z. `priorityOrder` is the priority option keys in registry order, low -> high.
function compareBy(key: TodoOrderKey, a: Entity, b: Entity, priorityOrder: readonly string[]): number {
  const af = todoFields(a)
  const bf = todoFields(b)
  switch (key) {
    case 'priority':
      return priorityOrder.indexOf(bf.priority) - priorityOrder.indexOf(af.priority)
    case 'due':
      if (af.due === bf.due) return 0
      if (af.due === undefined) return 1
      if (bf.due === undefined) return -1
      return af.due.localeCompare(bf.due)
    case 'name':
      return a.name.localeCompare(b.name)
  }
}

// ToDos matching the filter, in group order (Later by month), then the given order
// within each group or month.
export function selectTodos(entities: Entity[], filter: TodoFilter, order: TodoOrder, todayIsoValue: string): Entity[] {
  const matches = entities.filter((e) => {
    if (!hasFacet(e, TODO_FACET)) return false
    const fields = todoFields(e)
    if (filter.status !== 'all' && fields.status !== filter.status) return false
    if (filter.priority !== undefined && fields.priority !== filter.priority) return false
    if (filter.tag !== undefined && !fields.tagIds.includes(filter.tag)) return false
    if (!matchesDue(fields, filter, todayIsoValue)) return false
    return true
  })
  const priorityOrder = todoPriorityOptions(getFacet(TODO_FACET)).map((o) => o.key)
  const groupRank = (e: Entity) => TODO_GROUPS.findIndex((g) => g.key === todoGroup(todoFields(e), todayIsoValue))
  const monthOf = (e: Entity) => todoMonth(todoFields(e), todayIsoValue)
  return matches.sort((a, b) => {
    const group = groupRank(a) - groupRank(b)
    if (group !== 0) return group
    const months = (monthOf(a) ?? '').localeCompare(monthOf(b) ?? '')
    if (months !== 0) return months
    for (const [i, key] of order.keys.entries()) {
      const cmp = compareBy(key, a, b, priorityOrder)
      if (cmp !== 0) return i === 0 && order.reversed ? -cmp : cmp
    }
    return 0
  })
}

export interface TodoGroup {
  key: TodoGroupKey
  label: string
  todos: Entity[]
}

// Splits ToDos into their groups, in page order, keeping their order within each;
// groups with none are left out.
export function groupTodos(todos: Entity[], todayIsoValue: string): TodoGroup[] {
  return TODO_GROUPS.map(({ key, label }) => ({
    key,
    label,
    todos: todos.filter((e) => todoGroup(todoFields(e), todayIsoValue) === key),
  })).filter((g) => g.todos.length > 0)
}

export function todoDue(entity: Entity): string | undefined {
  if (!hasFacet(entity, TODO_FACET)) return undefined
  return todoFields(entity).due
}

export function useSetTodoStatus() {
  const { setSelect } = usePropertyWriters()
  return useCallback((entity: Entity, status: TodoStatus) => setSelect(entity, TODO_STATUS, status), [setSelect])
}

export function useTodos(filter: TodoFilter, order: TodoOrder): Entity[] {
  const entities = useEntitiesWithFacet(TODO_FACET)
  const today = useToday()
  return useMemo(() => selectTodos(entities, filter, order, today), [entities, filter, order, today])
}

export interface ParsedTodoInput {
  name: string
  // Tag names, without the '#', in first-seen order and deduplicated.
  tags: string[]
}

export type TodoInputResult = ({ ok: true } & ParsedTodoInput) | { ok: false; error: string }

// A tag is letters and digits, with single dots between them (`v1.2`).
const TAG = /^#([\p{L}\p{N}]+(?:\.[\p{L}\p{N}]+)*)$/u

// Quick-add text is `<name> #tag #tag…`: the name runs up to the first word
// starting with '#', and every word from there on must be a valid tag. Anything
// else refuses the whole line, so nothing is created from text it misread.
// "Buy milk #errands" -> "Buy milk", ['errands'].
export function parseTodoInput(text: string): TodoInputResult {
  const words = text.trim().split(/\s+/).filter(Boolean)
  const firstTag = words.findIndex((w) => w.startsWith('#'))
  const nameWords = firstTag === -1 ? words : words.slice(0, firstTag)
  const tagWords = firstTag === -1 ? [] : words.slice(firstTag)

  if (nameWords.length === 0) return { ok: false, error: 'Add a name before the tags.' }

  const tags: string[] = []
  for (const word of tagWords) {
    if (!word.startsWith('#')) return { ok: false, error: `Tags go last: "${word}" comes after a tag.` }
    const tag = TAG.exec(word)?.[1]
    if (!tag) return { ok: false, error: `"${word}" isn't a valid tag: use letters, digits and dots.` }
    if (!tags.includes(tag)) tags.push(tag)
  }

  return { ok: true, name: nameWords.join(' '), tags }
}

// Create a to-do Note from parsed quick-add text. Tags are resolved (or created)
// first and set in the same CreateEntity, so a failure creates nothing.
export function useQuickAddTodo() {
  const createEntity = useCreateEntity()
  return useCallback(
    async ({ name, tags }: ParsedTodoInput) => {
      const targets = await Promise.all(tags.map((tag) => getOrCreateEntityForMention('Tag', tag)))
      const properties = targets.length > 0 ? { tags: relationsValue(targets) } : undefined
      return createEntity('Note', { name, facets: [TODO_FACET], properties })
    },
    [createEntity],
  )
}
