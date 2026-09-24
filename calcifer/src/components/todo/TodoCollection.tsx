import { useNavigate } from '@tanstack/react-router'
import { useTodos, type TodoFilter, type TodoSearch, type TodoSort } from '~/model/todos'
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
      to: '/s/Todo',
      search: (prev: TodoSearch) => ({ ...prev, ...next }),
      replace: true,
    })

  return (
    <div className="flex flex-col gap-6 px-16 py-10">
      <h1 className="text-2xl font-semibold">To-dos</h1>
      <TodoQuickAdd />
      <TodoToolbar filter={filter} sort={sort} onChange={handleChange} />
      <TodoList entities={todos} />
    </div>
  )
}
