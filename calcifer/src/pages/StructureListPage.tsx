import { formatDistanceToNow } from 'date-fns'
import { Link } from '@tanstack/react-router'
import { listByStructure, useEntityStore } from '~/model/store'
import type { StructureId } from '~/model/structures'
import { STRUCTURES } from '~/model/structures'

export interface StructureListPageProps {
  structureId: string
}

export function StructureListPage({ structureId }: StructureListPageProps) {
  const entities = useEntityStore((s) => s.entities)

  if (!(structureId in STRUCTURES)) {
    return (
      <div className="flex flex-col gap-2 px-16 py-10">
        <h1 className="text-2xl font-semibold">Unknown structure</h1>
        <p className="text-sm text-muted-foreground">
          No structure registered for "{structureId}".
        </p>
      </div>
    )
  }

  const structure = STRUCTURES[structureId as StructureId]
  const items = listByStructure(entities, structureId as StructureId)

  const renderEmpty = () => (
    <p className="text-sm text-muted-foreground">No {structure.plural} yet.</p>
  )

  const renderList = () => (
    <ul className="flex flex-col gap-1">
      {items.map((entity) => (
        <li key={entity.id}>
          <Link
            to="/e/$id"
            params={{ id: entity.id }}
            className="flex items-center justify-between gap-4 rounded-md px-3 py-2 hover:bg-muted"
          >
            <span className="truncate">{entity.title}</span>
            <span className="shrink-0 text-xs text-muted-foreground">
              {formatDistanceToNow(new Date(entity.updatedAt), { addSuffix: true })}
            </span>
          </Link>
        </li>
      ))}
    </ul>
  )

  return (
    <div className="flex flex-col gap-6 px-16 py-10">
      <h1 className="text-2xl font-semibold">{structure.plural}</h1>
      {items.length === 0 ? renderEmpty() : renderList()}
    </div>
  )
}
