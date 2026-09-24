import { Link } from '@tanstack/react-router'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
import type { CalendarItem } from '~/model/calendar'
import type { Entity } from '~/model/store'
import { STRUCTURES, type StructureType } from '~/model/structures'
import { todoFields, useSetTodoStatus } from '~/model/todos'

interface CalendarItemRowProps {
  item: CalendarItem
}

// The calendar decides how each object type looks on a day. Add a case here
// when a structure deserves more than the generic icon + name row.
export function CalendarItemRow({ item }: CalendarItemRowProps) {
  switch (item.entity.structureType) {
    case 'Todo':
      return <TodoItemRow entity={item.entity} />
    default:
      return <EntityItemRow entity={item.entity} />
  }
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
  const meta = STRUCTURES[entity.structureType as StructureType]
  const Icon = meta?.icon
  const color = meta?.color ?? 'var(--muted-foreground)'

  return (
    <li>
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className="flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-muted"
      >
        {Icon && <Icon className="size-4 shrink-0" style={{ color }} aria-hidden />}
        <span className="truncate">{entity.name}</span>
      </Link>
    </li>
  )
}
