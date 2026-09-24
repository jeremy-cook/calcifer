import { useEffect, useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { TrashIcon } from '@phosphor-icons/react'
import { create as createMessage } from '@bufbuild/protobuf'
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
import { EntitySelectField } from '~/components/entity/EntitySelectField'
import { EntityRelationsField } from '~/components/entity/EntityRelationsField'
import { TodoStatusField } from '~/components/todo/TodoStatusField'
import { DailyNoteDateField } from '~/components/calendar/DailyNoteDateField'
import { BacklinksPanel } from '~/components/backlinks/BacklinksPanel'
import { useDeleteEntity, useEntity, useUpdateEntity, withName, withProperty, type Entity } from '~/model/store'
import { STRUCTURES, type StructureType, type PropertyDef, isNameEditable } from '~/model/structures'
import { EntityRefListSchema, type PropertyValue } from '@calcifer/proto/calcifer/v1/entities_pb'

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
  const updateEntity = useUpdateEntity()
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
    updateEntity(withName(entity, next))
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
  const updateEntity = useUpdateEntity()
  const structure = STRUCTURES[entity.structureType as StructureType]
  if (!structure || structure.properties.length === 0) return null

  const setProperty = (propertyId: string, value: PropertyValue['value'] | null) =>
    updateEntity(withProperty(entity, propertyId, value))

  // Properties whose editor isn't the generic one for their type, keyed "Structure.propertyId".
  const overrides: Record<string, () => React.ReactNode> = {
    'DailyNote.date': () => <DailyNoteDateField key="date" entity={entity} />,
    'Todo.status': () => <TodoStatusField key="status" entity={entity} />,
  }

  const renderProperty = (def: PropertyDef) => {
    const override = overrides[`${entity.structureType}.${def.id}`]
    if (override) return override()

    const property = entity.properties.find((p) => p.id === def.id)
    const value = property?.value?.value

    switch (def.type) {
      case 'richtext': {
        if (value?.case !== 'richtext') return null
        return <EntityRichTextField key={def.id} propertyId={def.id} propertyRef={value.value} />
      }
      case 'date': {
        const iso = value?.case === 'date' ? value.value : undefined
        return (
          <EntityDateField
            key={def.id}
            label={def.label ?? 'Date'}
            iso={iso}
            onChange={(next) => setProperty(def.id, { case: 'date', value: next })}
            onClear={iso ? () => setProperty(def.id, null) : undefined}
          />
        )
      }
      case 'select': {
        if (!def.options) return null
        const current = value?.case === 'select' ? value.value : (def.default ?? '')
        return (
          <EntitySelectField
            key={def.id}
            label={def.label ?? def.id}
            value={current}
            options={def.options}
            onChange={(next) => setProperty(def.id, { case: 'select', value: next })}
          />
        )
      }
      case 'relations': {
        if (!def.targetStructure) return null
        const refs = value?.case === 'relations' ? value.value.refs : []
        return (
          <EntityRelationsField
            key={def.id}
            label={def.label ?? def.id}
            targetStructure={def.targetStructure}
            refs={refs}
            onChange={(nextRefs) =>
              setProperty(
                def.id,
                nextRefs.length === 0
                  ? null
                  : { case: 'relations', value: createMessage(EntityRefListSchema, { refs: nextRefs }) },
              )
            }
          />
        )
      }
      default:
        return null
    }
  }

  return <>{structure.properties.map(renderProperty)}</>
}
