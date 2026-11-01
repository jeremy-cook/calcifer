import { Link } from '@tanstack/react-router'
import type { Entity } from '~/model/store'

export interface AlphabeticalIndexProps {
  entities: Entity[]
}

// Names that don't start with a letter share one '#' group, sorted first.
function groupByInitial(entities: Entity[]): [string, Entity[]][] {
  const sorted = [...entities].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
  const groups = new Map<string, Entity[]>()
  for (const entity of sorted) {
    const initial = entity.name.charAt(0).toLocaleUpperCase()
    const key = /\p{L}/u.test(initial) ? initial : '#'
    groups.set(key, [...(groups.get(key) ?? []), entity])
  }
  return [...groups]
}

export function AlphabeticalIndex({ entities }: AlphabeticalIndexProps) {
  return (
    <div className="columns-3 gap-8">
      {groupByInitial(entities).map(([initial, group]) => (
        <section key={initial} className="mb-2 break-inside-avoid">
          <h2 className="font-heading text-lg font-semibold text-primary">{initial}</h2>
          <ul>
            {group.map((entity) => (
              <li key={entity.id}>
                <Link
                  to="/e/$id"
                  params={{ id: entity.id }}
                  className="-mx-1.5 flex h-6 items-center truncate rounded-[5px] px-1.5 text-sm hover:bg-accent"
                >
                  {entity.name}
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  )
}
