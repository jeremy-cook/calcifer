import { Link } from '@tanstack/react-router'
import { CellSignalHighIcon, SunHorizonIcon } from '@phosphor-icons/react'
import { EmptyLine } from '~/components/empty-state/EmptyLine'
import { LoadingRows } from '~/components/loading/LoadingRows'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
import { formatShortDate, useToday } from '~/model/dates'
import { useReplicaReady, type Entity } from '~/model/store'
import {
  groupTodos,
  todoFields,
  useSetTodoStatus,
  useTodos,
  type TodoFilter,
  type TodoGroup,
  type TodoOrder,
} from '~/model/todos'

// Module-level so useTodos gets the same objects every render and keeps its memo.
const DUE_THIS_WEEK: TodoFilter = { status: 'open', due: ['overdue', 'today', 'week'] }
const BY_DUE: TodoOrder = { keys: ['due', 'priority', 'name'], reversed: false }

// Open ToDos due by the end of the week, overdue first.
export function HomeTodos() {
  const today = useToday()
  const todos = useTodos(DUE_THIS_WEEK, BY_DUE)
  const ready = useReplicaReady()

  if (!ready) return <LoadingRows count={3} />

  if (todos.length === 0) {
    return (
      <EmptyLine icon={SunHorizonIcon} color="var(--chart-4)">
        Nothing due this week.
      </EmptyLine>
    )
  }

  return (
    <div className="flex flex-col gap-2">
      {groupTodos(todos, today).map((group) => (
        <HomeTodoGroup key={group.key} group={group} today={today} />
      ))}
    </div>
  )
}

interface HomeTodoGroupProps {
  group: TodoGroup
  today: string
}

function HomeTodoGroup({ group, today }: HomeTodoGroupProps) {
  return (
    <div>
      <h3
        className={cn(
          'px-2.5 pb-0.5 text-xs font-semibold text-muted-foreground',
          group.key === 'overdue' && 'text-destructive',
        )}
      >
        {group.label}
      </h3>
      <ul className="flex flex-col">
        {group.todos.map((todo) => (
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
