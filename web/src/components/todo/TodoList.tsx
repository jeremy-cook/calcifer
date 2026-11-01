import { useMemo } from 'react'
import { useToday } from '~/model/dates'
import { useEntitiesOfType, type Entity } from '~/model/store'
import { todoFields } from '~/model/todos'
import { TodoRow } from './TodoRow'

export interface TodoListProps {
  entities: Entity[]
}

export function TodoList({ entities }: TodoListProps) {
  const today = useToday()
  const tags = useEntitiesOfType('Tag')
  const tagsById = useMemo(() => new Map(tags.map((tag) => [tag.id, tag])), [tags])

  if (entities.length === 0) {
    return <p className="text-sm text-muted-foreground">No ToDos match these filters.</p>
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
