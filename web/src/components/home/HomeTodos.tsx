import { Link } from '@tanstack/react-router'
import { CellSignalHighIcon } from '@phosphor-icons/react'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
import { formatShortDate, useToday } from '~/model/dates'
import type { Entity } from '~/model/store'
import { todoFields, useSetTodoStatus, useTodos, type TodoFilter, type TodoSort } from '~/model/todos'

// Module-level so useTodos gets the same objects every render and keeps its memo.
const OVERDUE: TodoFilter = { status: 'open', due: 'overdue' }
const TODAY: TodoFilter = { status: 'open', due: 'today' }
const WEEK: TodoFilter = { status: 'open', due: 'week' }
const BY_DUE: TodoSort = { by: 'due', dir: 'asc' }

// Open ToDos due by the end of the week, overdue first.
export function HomeTodos() {
  const today = useToday()
  const overdue = useTodos(OVERDUE, BY_DUE)
  const dueToday = useTodos(TODAY, BY_DUE)
  // "This week" counts today in; today's have their own group.
  const week = useTodos(WEEK, BY_DUE).filter((e) => todoFields(e).due !== today)

  if (overdue.length + dueToday.length + week.length === 0) {
    return <p className="px-2.5 text-sm text-muted-foreground">Nothing due this week.</p>
  }

  return (
    <div className="flex flex-col gap-2">
      <TodoGroup label="Overdue" todos={overdue} today={today} late />
      <TodoGroup label="Today" todos={dueToday} today={today} />
      <TodoGroup label="This week" todos={week} today={today} />
    </div>
  )
}

interface TodoGroupProps {
  label: string
  todos: Entity[]
  today: string
  late?: boolean
}

function TodoGroup({ label, todos, today, late }: TodoGroupProps) {
  if (todos.length === 0) return null

  return (
    <div>
      <h3 className={cn('px-2.5 pb-0.5 text-xs font-semibold text-muted-foreground', late && 'text-destructive')}>
        {label}
      </h3>
      <ul className="flex flex-col">
        {todos.map((todo) => (
          <HomeTodoRow key={todo.id} entity={todo} today={today} />
        ))}
      </ul>
    </div>
  )
}

interface HomeTodoRowProps {
  entity: Entity
  today: string
}

function HomeTodoRow({ entity, today }: HomeTodoRowProps) {
  const setTodoStatus = useSetTodoStatus()
  const { status, priority, due } = todoFields(entity)
  const overdue = due !== undefined && due < today

  const handleCheckedChange = (checked: boolean | 'indeterminate') => {
    setTodoStatus(entity, checked === true ? 'done' : 'open')
  }

  return (
    <li className="flex h-[30px] items-center gap-2.5 rounded-[5px] px-2.5 text-sm even:bg-card hover:bg-accent">
      <Checkbox
        checked={status === 'done'}
        onCheckedChange={handleCheckedChange}
        aria-label={`Mark "${entity.name}" done`}
      />
      <Link to="/e/$id" params={{ id: entity.id }} className="min-w-0 flex-1 truncate">
        {entity.name}
      </Link>
      {priority === 'high' && (
        <CellSignalHighIcon weight="fill" className="size-3.5 shrink-0 text-destructive" aria-label="High priority" />
      )}
      {due && (
        <span
          className={cn(
            'shrink-0 text-xs text-muted-foreground tabular-nums',
            overdue && 'font-semibold text-destructive',
          )}
        >
          {due === today ? 'Today' : formatShortDate(due)}
        </span>
      )}
    </li>
  )
}
