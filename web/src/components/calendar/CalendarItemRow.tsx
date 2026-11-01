import { Link } from '@tanstack/react-router'
import { format } from 'date-fns'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
import { timestampMsOrZero } from '~/model/dates'
import type { CalendarItem } from '~/model/calendar'
import { hasFacet, type Entity } from '~/model/store'
import { structurePresentation } from '~/model/structures'
import { TODO_FACET, todoFields, useSetTodoStatus } from '~/model/todos'

interface CalendarItemRowProps {
  item: CalendarItem
}

// The calendar decides how each entity looks on a day. Facets come first, so a
// to-do Note renders as a to-do. Branch here when a facet or structure deserves
// more than the generic icon + name row.
export function CalendarItemRow({ item }: CalendarItemRowProps) {
  // Made that day: the time it was made, on the right.
  const time = item.kind === 'created' ? format(new Date(timestampMsOrZero(item.entity.createdAt)), 'HH:mm') : undefined

  if (hasFacet(item.entity, TODO_FACET)) return <TodoItemRow entity={item.entity} time={time} />
  return <EntityItemRow entity={item.entity} time={time} />
}

// Compact rows banded on every other line, as in the Notes list.
const ROW_CLASS = 'flex h-[30px] items-center gap-2.5 rounded-[5px] px-2.5 text-sm even:bg-card hover:bg-accent'

interface RowTimeProps {
  time?: string
}

function RowTime({ time }: RowTimeProps) {
  if (!time) return null
  return <span className="ml-auto shrink-0 text-xs text-muted-foreground tabular-nums">{time}</span>
}

interface TodoItemRowProps {
  entity: Entity
  time?: string
}

function TodoItemRow({ entity, time }: TodoItemRowProps) {
  const setTodoStatus = useSetTodoStatus()
  const done = todoFields(entity).status === 'done'

  const handleCheckedChange = (checked: boolean | 'indeterminate') => {
    setTodoStatus(entity, checked === true ? 'done' : 'open')
  }

  return (
    <li className={ROW_CLASS}>
      <Checkbox checked={done} onCheckedChange={handleCheckedChange} aria-label={`Mark "${entity.name}" done`} />
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className={cn('min-w-0 truncate', done && 'text-muted-foreground line-through')}
      >
        {entity.name}
      </Link>
      <RowTime time={time} />
    </li>
  )
}

interface EntityItemRowProps {
  entity: Entity
  time?: string
}

function EntityItemRow({ entity, time }: EntityItemRowProps) {
  const { icon: Icon, color } = structurePresentation(entity.structureType)

  return (
    <li className="rounded-[5px] even:bg-card">
      <Link to="/e/$id" params={{ id: entity.id }} className={ROW_CLASS}>
        <Icon weight="duotone" className="size-4 shrink-0" style={{ color }} aria-hidden />
        <span className="min-w-0 truncate">{entity.name}</span>
        <RowTime time={time} />
      </Link>
    </li>
  )
}
