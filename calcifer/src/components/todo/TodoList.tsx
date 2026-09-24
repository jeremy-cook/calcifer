import type { Entity } from '~/model/store'
import { TodoRow } from './TodoRow'

export interface TodoListProps {
  entities: Entity[]
}

export function TodoList({ entities }: TodoListProps) {
  if (entities.length === 0) {
    return <p className="text-sm text-muted-foreground">No to-dos match these filters.</p>
  }
  return (
    <ul className="flex flex-col gap-0.5">
      {entities.map((entity) => (
        <TodoRow key={entity.id} entity={entity} />
      ))}
    </ul>
  )
}
