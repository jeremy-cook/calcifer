import { useNavigate } from '@tanstack/react-router'
import { DEFAULT_TODO_FILTER, useTodos, type TodoFilter, type TodoSearch, type TodoSort } from '~/model/todos'
import { TodoQuickAdd } from './TodoQuickAdd'
import { TodoToolbar } from './TodoToolbar'
import { TodoList } from './TodoList'

export interface TodoCollectionProps {
  filter: TodoFilter
  sort: TodoSort
}

export function TodoCollection({ filter, sort }: TodoCollectionProps) {
  const navigate = useNavigate()
  const todos = useTodos(filter, sort)

  const handleChange = (next: Partial<TodoSearch>) =>
    void navigate({
      to: '/v/todos',
      search: (prev: TodoSearch) => ({ ...prev, ...next }),
      replace: true,
    })

  const filtered =
    filter.status !== DEFAULT_TODO_FILTER.status ||
    filter.priority !== undefined ||
    filter.tag !== undefined ||
    filter.due !== undefined

  const clearFilters = () => handleChange({ status: undefined, priority: undefined, tag: undefined, due: undefined })

  return (
    <div className="flex flex-col gap-6 px-16 py-10">
      <h1 className="font-heading text-3xl font-semibold text-heading">ToDo</h1>
      <TodoQuickAdd />
      <TodoToolbar filter={filter} sort={sort} onChange={handleChange} />
      <TodoList entities={todos} filtered={filtered} onClearFilters={clearFilters} />
    </div>
  )
}
