import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { TrashIcon } from '@phosphor-icons/react'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '~/components/ui/alert-dialog'
import { Button } from '~/components/ui/button'
import { Input } from '~/components/ui/input'
import { EntityRichTextField } from '~/components/entity/EntityRichTextField'
import { EntityDateField } from '~/components/entity/EntityDateField'
import { BacklinksPanel } from '~/components/backlinks/BacklinksPanel'
import {
  useDeleteEntity,
  useEntity,
  useMoveDailyNote,
  useRenameEntity,
  type Entity,
} from '~/model/store'
import { STRUCTURES, type StructureType, isNameEditable } from '~/model/structures'
import type { Property } from '@calcifer/proto/calcifer/v1/entities_pb'

export const Route = createFileRoute('/e/$id')({
  component: function EntityRoute() {
    const { id } = Route.useParams()
    const { data: entity, isPending } = useEntity(id)
    if (isPending) return null
    if (!entity) return <EntityNotFound />

    return (
      <div className="flex h-full flex-col">
        <EntityHeader entity={entity} />
        <div className="min-h-0 flex-1 overflow-y-auto">
          <EntityProperties entity={entity} />
          <div className="border-t border-border px-12 py-6">
            <BacklinksPanel entityId={entity.id} />
          </div>
        </div>
      </div>
    )
  },
})

function EntityNotFound() {
  return (
    <div className="flex flex-col gap-2 px-16 py-10">
      <h1 className="text-2xl font-semibold">Entity not found</h1>
      <p className="text-sm text-muted-foreground">This entity doesn't exist or was deleted.</p>
    </div>
  )
}

interface EntityHeaderProps {
  entity: Entity
}

function EntityHeader({ entity }: EntityHeaderProps) {
  const rename = useRenameEntity()
  const deleteEntity = useDeleteEntity()
  const navigate = useNavigate()

  const structureName = STRUCTURES[entity.structureType as StructureType]?.name ?? entity.structureType
  const isDefaultName = entity.name === `Untitled ${structureName}`
  const nameEditable = isNameEditable(entity.structureType)

  const handleDelete = () => {
    deleteEntity(entity.id)
    void navigate({ to: '/' })
  }

  const renderTitle = () =>
    nameEditable ? (
      <Input
        value={entity.name}
        autoFocus={isDefaultName}
        onChange={(e) => rename(entity, e.target.value)}
        className="h-10 border-0 bg-transparent text-2xl font-semibold shadow-none focus-visible:ring-0"
        aria-label="Entity title"
      />
    ) : (
      <h1 className="flex h-10 flex-1 items-center px-3 text-2xl font-semibold">{entity.name}</h1>
    )

  return (
    <div className="flex items-center gap-2 border-b border-border px-12 py-6">
      {renderTitle()}
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button variant="ghost" size="icon" aria-label="Delete entity">
            <TrashIcon />
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {structureName.toLowerCase()}?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{entity.name}" and its content.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete}>Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

interface EntityPropertiesProps {
  entity: Entity
}

function EntityProperties({ entity }: EntityPropertiesProps) {
  const moveDailyNote = useMoveDailyNote()
  if (entity.properties.length === 0) return null

  const renderProperty = (property: Property) => {
    const value = property.value?.value
    if (!value) return null
    switch (value.case) {
      case 'richtext':
        return <EntityRichTextField key={property.id} propertyId={property.id} propertyRef={value.value} />
      case 'date':
        if (entity.structureType !== 'DailyNote') return null
        return (
          <EntityDateField
            key={property.id}
            label="Date"
            iso={value.value}
            onChange={(iso) => moveDailyNote(entity, iso)}
          />
        )
      default:
        return null
    }
  }

  return <>{entity.properties.map(renderProperty)}</>
}
