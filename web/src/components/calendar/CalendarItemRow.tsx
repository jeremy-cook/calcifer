import { Link } from '@tanstack/react-router'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
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
  if (hasFacet(item.entity, TODO_FACET)) return <TodoItemRow entity={item.entity} />
  return <EntityItemRow entity={item.entity} />
}

interface TodoItemRowProps {
  entity: Entity
}

function TodoItemRow({ entity }: TodoItemRowProps) {
  const setTodoStatus = useSetTodoStatus()
  const done = todoFields(entity).status === 'done'

  const handleCheckedChange = (checked: boolean | 'indeterminate') => {
    setTodoStatus(entity, checked === true ? 'done' : 'open')
  }

  return (
    <li className="flex items-center gap-2 rounded-md px-2 py-1 hover:bg-muted">
      <Checkbox checked={done} onCheckedChange={handleCheckedChange} aria-label={`Mark "${entity.name}" done`} />
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className={cn('truncate text-sm', done && 'text-muted-foreground line-through')}
      >
        {entity.name}
      </Link>
    </li>
  )
}

interface EntityItemRowProps {
  entity: Entity
}

function EntityItemRow({ entity }: EntityItemRowProps) {
  const { icon: Icon, color } = structurePresentation(entity.structureType)

  return (
    <li>
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className="flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-muted"
      >
        <Icon className="size-4 shrink-0" style={{ color }} aria-hidden />
        <span className="truncate">{entity.name}</span>
      </Link>
    </li>
  )
}
