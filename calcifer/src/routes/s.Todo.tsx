import { createFileRoute } from '@tanstack/react-router'
import { TodoCollection } from '~/components/todo/TodoCollection'
import {
  DEFAULT_TODO_FILTER,
  DEFAULT_TODO_SORT,
  TODO_DUE_FILTER_OPTIONS,
  TODO_SORT_DIR_OPTIONS,
  TODO_SORT_OPTIONS,
  isOptionKey,
  todoPriorityOptions,
  todoStatusFilterOptions,
  type TodoFilter,
  type TodoSearch,
  type TodoSort,
} from '~/model/todos'
import { getStructure } from '~/model/structures'

// Static segment: this route takes precedence over /s/$structureType for Todo.
export const Route = createFileRoute('/s/Todo')({
  // Status/priority keys come from the registry, read per call (it has loaded
  // before the router renders), never captured at import.
  validateSearch: (search: Record<string, unknown>): TodoSearch => {
    const todo = getStructure('Todo')
    return {
      status: isOptionKey(todoStatusFilterOptions(todo), search.status) ? search.status : undefined,
      priority: isOptionKey(todoPriorityOptions(todo), search.priority) ? search.priority : undefined,
      tag: typeof search.tag === 'string' ? search.tag : undefined,
      due: isOptionKey(TODO_DUE_FILTER_OPTIONS, search.due) ? search.due : undefined,
      sort: isOptionKey(TODO_SORT_OPTIONS, search.sort) ? search.sort : undefined,
      dir: isOptionKey(TODO_SORT_DIR_OPTIONS, search.dir) ? search.dir : undefined,
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
    const sort: TodoSort = {
      by: search.sort ?? DEFAULT_TODO_SORT.by,
      dir: search.dir ?? DEFAULT_TODO_SORT.dir,
    }
    return <TodoCollection filter={filter} sort={sort} />
  },
})
