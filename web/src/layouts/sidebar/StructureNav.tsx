import { Link } from '@tanstack/react-router'
import { cn } from '~/lib/utils'
import { facetPresentation, structurePresentation, useFacets, useStructures } from '~/model/structures'
import { NAV_ROW_ACTIVE_CLASS, NAV_ROW_CLASS, NAV_ROW_RAIL_CLASS } from './nav-row'

export interface StructureNavProps {
  rail: boolean
}

export function StructureNav({ rail }: StructureNavProps) {
  const structures = useStructures()
  // Facets with a list page, after the structures.
  const facetPages = useFacets().flatMap((f) => {
    const { icon, color, navLabel, route } = facetPresentation(f.id)
    return route ? [{ id: f.id, icon, color, navLabel, route }] : []
  })

  return (
    <div className="flex flex-col gap-0.5">
      {structures.map((s) => {
        const { icon: IconComponent, color } = structurePresentation(s.type)
        return (
          <Link
            key={s.type}
            to="/s/$structureType"
            params={{ structureType: s.type }}
            title={rail ? s.plural : undefined}
            className={cn(NAV_ROW_CLASS, rail && NAV_ROW_RAIL_CLASS)}
            activeProps={{ className: NAV_ROW_ACTIVE_CLASS }}
          >
            <IconComponent weight="duotone" style={{ color }} />
            <span className="truncate">{s.plural}</span>
          </Link>
        )
      })}
      {facetPages.map(({ id, icon: IconComponent, color, navLabel, route }) => (
        <Link
          key={id}
          to={route}
          title={rail ? navLabel : undefined}
          className={cn(NAV_ROW_CLASS, rail && NAV_ROW_RAIL_CLASS)}
          activeProps={{ className: NAV_ROW_ACTIVE_CLASS }}
        >
          <IconComponent weight="duotone" style={{ color }} />
          <span className="truncate">{navLabel}</span>
        </Link>
      ))}
    </div>
  )
}
