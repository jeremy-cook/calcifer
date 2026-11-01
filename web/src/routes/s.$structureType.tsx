import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { FileDashedIcon, PlusIcon } from '@phosphor-icons/react'
import { EmptyState } from '~/components/empty-state/EmptyState'
import { AlphabeticalIndex } from '~/components/structure-list/AlphabeticalIndex'
import { EntityRows } from '~/components/structure-list/EntityRows'
import { Button } from '~/components/ui/button'
import { listByStructure, useAllEntities, useCreateEntity } from '~/model/store'
import { structurePresentation, useStructure } from '~/model/structures'

export const Route = createFileRoute('/s/$structureType')({
  component: function StructureListRoute() {
    const { structureType } = Route.useParams()
    const navigate = useNavigate()
    const createEntity = useCreateEntity()
    const entities = useAllEntities()
    const structure = useStructure(structureType)

    if (!structure) {
      return (
        <div className="px-16 py-10">
          <EmptyState
            icon={FileDashedIcon}
            color="var(--muted-foreground)"
            title="Nothing here"
            description={`There's no structure called "${structureType}".`}
            action={
              <Button variant="outline" asChild>
                <Link to="/">Go home</Link>
              </Button>
            }
          />
        </div>
      )
    }

    const items = listByStructure(entities, structureType)
    const presentation = structurePresentation(structureType)
    const alphabetical = presentation.listLayout === 'alphabetical'

    const handleCreate = async () => {
      const entity = await createEntity(structureType)
      void navigate({ to: '/e/$id', params: { id: entity.id }, state: { justCreated: true } })
    }

    // Structures made elsewhere (daily notes, by date) send the reader to where they're made.
    const renderEmptyAction = () =>
      structure.creatable ? (
        <Button onClick={() => void handleCreate()}>
          <PlusIcon weight="bold" />
          New {structure.name.toLowerCase()}
        </Button>
      ) : (
        <Button variant="outline" asChild>
          <Link to="/">Open today</Link>
        </Button>
      )

    const renderEmpty = () => (
      <EmptyState
        icon={presentation.icon}
        color={presentation.color}
        title="A blank shelf"
        description={`${structure.plural} you make show up here, ${alphabetical ? 'A to Z' : 'newest first'}.`}
        action={renderEmptyAction()}
      />
    )

    const renderList = () => (alphabetical ? <AlphabeticalIndex entities={items} /> : <EntityRows entities={items} />)

    return (
      <div className="flex flex-col gap-4 px-16 py-10">
        <h1 className="font-heading text-3xl font-semibold text-heading">{structure.plural}</h1>
        {items.length === 0 ? renderEmpty() : renderList()}
      </div>
    )
  },
})
