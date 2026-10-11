import { useToday } from '~/model/dates'
import { useEntitiesById, type Entity } from '~/model/store'
import { todoFields } from '~/model/todos'
import { TodoRow } from './TodoRow'

export interface TodoListProps {
  entities: Entity[]
}

export function TodoList({ entities }: TodoListProps) {
  const today = useToday()
  const entitiesById = useEntitiesById()

  if (entities.length === 0) {
    return <p className="text-sm text-muted-foreground">No to-dos match these filters.</p>
  }

  const resolveTags = (entity: Entity) =>
    todoFields(entity)
      .tagIds.map((id) => entitiesById.get(id))
      .filter((e): e is Entity => e !== undefined)

  return (
    <ul className="flex flex-col gap-0.5">
      {entities.map((entity) => (
        <TodoRow key={entity.id} entity={entity} today={today} tags={resolveTags(entity)} />
      ))}
    </ul>
  )
}
