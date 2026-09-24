import { memo } from 'react'
import { Link } from '@tanstack/react-router'
import { Checkbox } from '~/components/ui/checkbox'
import { Badge } from '~/components/ui/badge'
import { cn } from '~/lib/utils'
import { formatLongDate } from '~/model/dates'
import type { Entity } from '~/model/store'
import { todoFields, useSetTodoStatus } from '~/model/todos'
import { PriorityBadge } from './PriorityBadge'

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

export const TodoRow = memo(function TodoRow({ entity, today, tags }: TodoRowProps) {
  const setTodoStatus = useSetTodoStatus()
  const { status, priority, due } = todoFields(entity)
  const done = status === 'done'
  const overdue = !done && due !== undefined && due < today

  const handleCheckedChange = (checked: boolean | 'indeterminate') => {
    setTodoStatus(entity, checked === true ? 'done' : 'open')
  }

  return (
    <li className="flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-muted">
      <Checkbox checked={done} onCheckedChange={handleCheckedChange} aria-label={`Mark "${entity.name}" done`} />
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className={cn('min-w-0 flex-1 truncate text-sm', done && 'text-muted-foreground line-through')}
      >
        {entity.name}
      </Link>
      <PriorityBadge priority={priority} />
      {due && (
        <span className={cn('shrink-0 text-xs text-muted-foreground', overdue && 'text-destructive')}>
          {formatLongDate(due)}
        </span>
      )}
      {tags.length > 0 && (
        <div className="flex shrink-0 items-center gap-1">
          {tags.map((tag) => (
            <Badge key={tag.id} variant="outline">
              {tag.name}
            </Badge>
          ))}
        </div>
      )}
    </li>
  )
}, arePropsEqual)
