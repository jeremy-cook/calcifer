import { createFileRoute } from '@tanstack/react-router'
import { AlphabeticalIndex } from '~/components/structure-list/AlphabeticalIndex'
import { EntityRows } from '~/components/structure-list/EntityRows'
import { listByStructure, useAllEntities } from '~/model/store'
import { structurePresentation, useStructure } from '~/model/structures'

export const Route = createFileRoute('/s/$structureType')({
  component: function StructureListRoute() {
    const { structureType } = Route.useParams()
    const entities = useAllEntities()
    const structure = useStructure(structureType)

    if (!structure) {
      return (
        <div className="flex flex-col gap-2 px-16 py-10">
          <h1 className="text-2xl font-semibold">Unknown structure</h1>
          <p className="text-sm text-muted-foreground">No structure registered for "{structureType}".</p>
        </div>
      )
    }

    const items = listByStructure(entities, structureType)

    const renderEmpty = () => <p className="text-sm text-muted-foreground">No {structure.plural} yet.</p>

    const renderList = () =>
      structurePresentation(structureType).listLayout === 'alphabetical' ? (
        <AlphabeticalIndex entities={items} />
      ) : (
        <EntityRows entities={items} />
      )

    return (
      <div className="flex flex-col gap-4 px-16 py-10">
        <h1 className="font-heading text-3xl font-semibold text-heading">{structure.plural}</h1>
        {items.length === 0 ? renderEmpty() : renderList()}
      </div>
    )
  },
})
