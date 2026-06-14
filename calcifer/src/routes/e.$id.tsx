import { useEffect, useState } from 'react'
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
  const deleteEntity = useDeleteEntity()
  const navigate = useNavigate()

  const structureName = STRUCTURES[entity.structureType as StructureType]?.name ?? entity.structureType
  const nameEditable = isNameEditable(entity.structureType)

  const handleDelete = () => {
    deleteEntity(entity.id)
    void navigate({ to: '/' })
  }

  const renderTitle = () =>
    nameEditable ? (
      <EntityTitleInput entity={entity} structureName={structureName} />
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

interface EntityTitleInputProps {
  entity: Entity
  structureName: string
}

// Local input state seeded from entity.name so the optimistic rename round-trip
// (onMutate's async `await cancelQueries` -> setQueryData) can never visibly
// revert the field mid-typing: keystrokes update local state synchronously, and
// the effect below only re-syncs when entity.name actually changes.
function EntityTitleInput({ entity, structureName }: EntityTitleInputProps) {
  const rename = useRenameEntity()
  const [value, setValue] = useState(entity.name)

  const isDefaultName = entity.name === `Untitled ${structureName}`

  // Sync the canonical name down only when it actually differs (an external
  // rename, e.g. another client). Our own keystrokes already set `value` first
  // and the optimistic cache update makes entity.name match, so this never fires
  // for self-edits and the field cannot revert mid-typing.
  useEffect(() => {
    setValue(entity.name)
  }, [entity.name])

  const handleChange = (next: string) => {
    setValue(next)
    rename(entity, next)
  }

  return (
    <Input
      value={value}
      autoFocus={isDefaultName}
      onChange={(e) => handleChange(e.target.value)}
      className="h-10 border-0 bg-transparent text-2xl font-semibold shadow-none focus-visible:ring-0"
      aria-label="Entity title"
    />
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
