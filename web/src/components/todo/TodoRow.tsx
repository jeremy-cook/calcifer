import { memo } from 'react'
import { Link } from '@tanstack/react-router'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
import { formatShortDate } from '~/model/dates'
import type { Entity } from '~/model/store'
import { optionLabel, propertyDef, useFacet } from '~/model/structures'
import { TODO_FACET, TODO_PRIORITY, todoFields, useSetTodoStatus } from '~/model/todos'
import { PRIORITY_CLASSES, PRIORITY_ICONS } from './priority'

export interface TodoRowProps {
  entity: Entity
  today: string
  tags: Entity[]
}

// TodoList resolves tags into a fresh array on every render, so compare them
// element-wise; rows re-render only when their own entity, tags or the date change.
function arePropsEqual(prev: TodoRowProps, next: TodoRowProps) {
  return (
    prev.entity === next.entity &&
    prev.today === next.today &&
    prev.tags.length === next.tags.length &&
    prev.tags.every((tag, i) => tag === next.tags[i])
  )
}

// Fixed columns, so tags, priority and due date line up down the page.
export const TodoRow = memo(function TodoRow({ entity, today, tags }: TodoRowProps) {
  const setTodoStatus = useSetTodoStatus()
  const priorityDef = propertyDef(useFacet(TODO_FACET), TODO_PRIORITY)
  const { status, priority, due } = todoFields(entity)
  const done = status === 'done'
  const overdue = !done && due !== undefined && due < today
  const PriorityIcon = PRIORITY_ICONS[priority]

  const handleCheckedChange = (checked: boolean | 'indeterminate') => {
    setTodoStatus(entity, checked === true ? 'done' : 'open')
  }

  const renderPriority = () =>
    priority === 'none' ? (
      <span className="text-muted-foreground/60">—</span>
    ) : (
      <span className={cn('flex items-center gap-1.5', done ? 'text-muted-foreground' : PRIORITY_CLASSES[priority])}>
        {PriorityIcon && <PriorityIcon weight="fill" className="size-3.5 shrink-0" aria-hidden />}
        <span className="text-xs text-secondary-foreground">{optionLabel(priorityDef, priority)}</span>
      </span>
    )

  return (
    <li className="grid h-[30px] grid-cols-[1rem_minmax(0,1fr)_minmax(0,13rem)_5.5rem_4.5rem] items-center gap-3 rounded-[5px] px-2.5 text-sm even:bg-card hover:bg-accent">
      <Checkbox checked={done} onCheckedChange={handleCheckedChange} aria-label={`Mark "${entity.name}" done`} />
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className={cn('truncate', done && 'text-muted-foreground line-through')}
      >
        {entity.name}
      </Link>
      <span className="flex gap-2 overflow-hidden text-xs font-semibold whitespace-nowrap text-chart-2">
        {tags.map((tag) => (
          <Link key={tag.id} to="/e/$id" params={{ id: tag.id }} className="hover:underline">
            <span className="opacity-60">#</span>
            {tag.name}
          </Link>
        ))}
      </span>
      {renderPriority()}
      <span
        className={cn(
          'text-right text-xs whitespace-nowrap text-muted-foreground tabular-nums',
          overdue && 'font-semibold text-destructive',
          !done && due === today && 'font-semibold text-primary',
        )}
      >
        {due && (due === today ? 'Today' : formatShortDate(due))}
      </span>
    </li>
  )
}, arePropsEqual)
