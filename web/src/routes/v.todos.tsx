import { createFileRoute } from '@tanstack/react-router'
import { TodoCollection } from '~/components/todo/TodoCollection'
import {
  DEFAULT_TODO_FILTER,
  DEFAULT_TODO_ORDER,
  TODO_FACET,
  isOptionKey,
  isTodoDueKey,
  isTodoOrder,
  todoPriorityOptions,
  todoStatusFilterOptions,
  type TodoFilter,
  type TodoOrder,
  type TodoSearch,
} from '~/model/todos'
import { getFacet } from '~/model/structures'

export const Route = createFileRoute('/v/todos')({
  // Status/priority keys come from the registry, read per call (it has loaded
  // before the router renders), never captured at import.
  validateSearch: (search: Record<string, unknown>): TodoSearch => {
    const todo = getFacet(TODO_FACET)
    const due = Array.isArray(search.due) ? search.due.filter(isTodoDueKey) : []
    return {
      status: isOptionKey(todoStatusFilterOptions(todo), search.status) ? search.status : undefined,
      priority: isOptionKey(todoPriorityOptions(todo), search.priority) ? search.priority : undefined,
      tag: typeof search.tag === 'string' ? search.tag : undefined,
      due: due.length > 0 ? due : undefined,
      order: isTodoOrder(search.order) ? search.order : undefined,
      reversed: search.reversed === true ? true : undefined,
    }
  },
  component: function TodoRoute() {
    const search = Route.useSearch()
    const filter: TodoFilter = {
      status: search.status ?? DEFAULT_TODO_FILTER.status,
      priority: search.priority,
      tag: search.tag,
      due: search.due,
    }
    const order: TodoOrder = {
      keys: search.order ?? DEFAULT_TODO_ORDER.keys,
      reversed: search.reversed ?? DEFAULT_TODO_ORDER.reversed,
    }
    return <TodoCollection filter={filter} order={order} />
  },
})
