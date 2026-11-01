import { useMemo } from 'react'
import { CheckCircleIcon, FunnelIcon } from '@phosphor-icons/react'
import { EmptyState } from '~/components/empty-state/EmptyState'
import { Button } from '~/components/ui/button'
import { useToday } from '~/model/dates'
import { useEntitiesOfType, type Entity } from '~/model/store'
import { todoFields } from '~/model/todos'
import { TodoRow } from './TodoRow'

export interface TodoListProps {
  entities: Entity[]
  // Whether the filters differ from the defaults, and how to reset them.
  filtered: boolean
  onClearFilters: () => void
}

export function TodoList({ entities, filtered, onClearFilters }: TodoListProps) {
  const today = useToday()
  const tags = useEntitiesOfType('Tag')
  const tagsById = useMemo(() => new Map(tags.map((tag) => [tag.id, tag])), [tags])

  if (entities.length === 0 && filtered) {
    return (
      <EmptyState
        icon={FunnelIcon}
        color="var(--chart-4)"
        title="Nothing matches"
        description="No ToDos match these filters."
        action={
          <Button variant="outline" onClick={onClearFilters}>
            Clear filters
          </Button>
        }
      />
    )
  }

  if (entities.length === 0) {
    return (
      <EmptyState
        icon={CheckCircleIcon}
        color="var(--chart-4)"
        title="All clear"
        description="No open ToDos. Add one above."
      />
    )
  }

  const resolveTags = (entity: Entity) =>
    todoFields(entity)
      .tagIds.map((id) => tagsById.get(id))
      .filter((e): e is Entity => e !== undefined)

  return (
    <ul className="flex flex-col">
      {entities.map((entity) => (
        <TodoRow key={entity.id} entity={entity} today={today} tags={resolveTags(entity)} />
      ))}
    </ul>
  )
}
