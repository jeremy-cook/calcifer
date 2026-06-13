import { useMemo } from 'react'
import { Link } from '@tanstack/react-router'
import { entitiesByDate, useAllEntities, type Entity } from '~/model/store'
import { STRUCTURES, type StructureType } from '~/model/structures'
import { cn } from '~/lib/utils'

interface DateReferencesSectionProps {
  iso: string
}

export function DateReferencesSection({ iso }: DateReferencesSectionProps) {
  const entities = useAllEntities()
  const filtered = useMemo(
    () => entitiesByDate(entities, iso).filter((e) => e.structureType !== 'DailyNote'),
    [entities, iso],
  )

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-base font-semibold">Date references</h2>
      {filtered.length === 0 ? <EmptyState /> : <List entities={filtered} />}
    </section>
  )
}

function EmptyState() {
  return <p className="text-sm text-muted-foreground">No date references.</p>
}

interface ListProps {
  entities: Entity[]
}

function List({ entities }: ListProps) {
  return (
    <ul className="flex flex-col gap-0.5">
      {entities.map((entity) => (
        <Row key={entity.id} entity={entity} />
      ))}
    </ul>
  )
}

interface RowProps {
  entity: Entity
}

function Row({ entity }: RowProps) {
  const meta = STRUCTURES[entity.structureType as StructureType]
  const Icon = meta?.icon
  const color = meta?.color ?? 'var(--muted-foreground)'

  return (
    <li>
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className={cn('flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-muted')}
      >
        {Icon && <Icon className="size-4 shrink-0" style={{ color }} aria-hidden />}
        <span className="truncate">{entity.name}</span>
      </Link>
    </li>
  )
}
