import { Link } from '@tanstack/react-router'
import { Checkbox } from '~/components/ui/checkbox'
import { Badge } from '~/components/ui/badge'
import { cn } from '~/lib/utils'
import { todayIso, formatLongDate } from '~/model/dates'
import { useAllEntities, type Entity } from '~/model/store'
import { todoFields, useSetTodoStatus } from '~/model/todos'
import { PriorityBadge } from './PriorityBadge'

export interface TodoRowProps {
  entity: Entity
}

export function TodoRow({ entity }: TodoRowProps) {
  const setTodoStatus = useSetTodoStatus()
  const entities = useAllEntities()
  const { status, priority, due, tagIds } = todoFields(entity)
  const done = status === 'done'
  const overdue = !done && due !== undefined && due < todayIso()
  const tags = tagIds.map((id) => entities.find((e) => e.id === id)).filter((e): e is Entity => e !== undefined)

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
}
