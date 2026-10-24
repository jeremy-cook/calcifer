import { Link } from '@tanstack/react-router'
import { facetPresentation, structurePresentation, useFacets, useStructures } from '~/model/structures'

const ROW_CLASS =
  'flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground hover:bg-muted [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground'

export function StructureNav() {
  const structures = useStructures()
  // Facets with a list page, after the structures.
  const facetPages = useFacets().flatMap((f) => {
    const { icon, navLabel, route } = facetPresentation(f.id)
    return route ? [{ id: f.id, icon, navLabel, route }] : []
  })

  return (
    <div className="flex flex-col gap-0.5">
      {structures.map((s) => {
        const IconComponent = structurePresentation(s.type).icon
        return (
          <Link
            key={s.type}
            to="/s/$structureType"
            params={{ structureType: s.type }}
            className={ROW_CLASS}
            activeProps={{ className: 'bg-muted font-medium' }}
          >
            <IconComponent />
            <span className="truncate">{s.plural}</span>
          </Link>
        )
      })}
      {facetPages.map(({ id, icon: IconComponent, navLabel, route }) => (
        <Link key={id} to={route} className={ROW_CLASS} activeProps={{ className: 'bg-muted font-medium' }}>
          <IconComponent />
          <span className="truncate">{navLabel}</span>
        </Link>
      ))}
    </div>
  )
}
