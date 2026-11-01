import { Link } from '@tanstack/react-router'
import { formatDistanceToNow } from 'date-fns'
import { entityUpdatedAtDate, type Entity } from '~/model/store'
import { structurePresentation } from '~/model/structures'

export interface EntityRowsProps {
  entities: Entity[]
}

// Compact rows banded on every other line, like green-bar printout paper.
export function EntityRows({ entities }: EntityRowsProps) {
  return (
    <ul className="flex flex-col">
      {entities.map((entity) => {
        const { icon: Icon, color } = structurePresentation(entity.structureType)
        return (
          <li key={entity.id} className="rounded-[5px] even:bg-card">
            <Link
              to="/e/$id"
              params={{ id: entity.id }}
              className="flex h-[30px] items-center gap-2.5 rounded-[5px] px-2.5 text-sm hover:bg-accent"
            >
              <Icon weight="duotone" className="size-4 shrink-0" style={{ color }} aria-hidden />
              <span className="min-w-0 flex-1 truncate">{entity.name}</span>
              <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                {formatDistanceToNow(entityUpdatedAtDate(entity), { addSuffix: true })}
              </span>
            </Link>
          </li>
        )
      })}
    </ul>
  )
}
