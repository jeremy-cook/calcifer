import { useCallback, useEffect, useRef, useState } from 'react'
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
import { PropertyKind, isNameEditable, useStructure, type PropertyDef } from '~/model/structures'
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
  const structure = useStructure(entity.structureType)

  const structureName = structure?.name ?? entity.structureType
  const nameEditable = isNameEditable(entity.structureType)

  const handleDelete = () => {
    deleteEntity(entity.id)
    void navigate({ to: '/' })
  }

  const renderTitle = () =>
    nameEditable ? (
      <EntityTitleInput key={entity.id} entity={entity} structureName={structureName} />
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

// Renames are debounced so a typing burst sends one Update. While the field is
// focused, `value` is the source of truth and entity.name is never synced into
// it: watch-stream echoes of earlier writes can land after a later optimistic
// update and briefly move entity.name back to an older prefix. Outside of focus,
// external renames (another client, the MCP agent) are adopted during render.
function EntityTitleInput({ entity, structureName }: EntityTitleInputProps) {
  const updateEntity = useUpdateEntity()
  const [value, setValue] = useState(entity.name)
  const [focused, setFocused] = useState(false)
  const [syncedName, setSyncedName] = useState(entity.name)
  const latestEntity = useRef(entity)
  const pendingName = useRef<string | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  if (entity.name !== syncedName) {
    setSyncedName(entity.name)
    if (!focused) setValue(entity.name)
  }

  // The debounced write must build on the latest entity, not the one captured
  // when the timer started, or it would undo a concurrent property change.
  useEffect(() => {
    latestEntity.current = entity
  }, [entity])

  const flush = useCallback(() => {
    clearTimeout(timer.current)
    timer.current = undefined
    const name = pendingName.current
    if (name === null) return
    pendingName.current = null
    updateEntity(withName(latestEntity.current, name))
  }, [updateEntity])

  // Unmount (including navigation to another entity, via the key in
  // EntityHeader) must not drop the last characters typed.
  useEffect(() => flush, [flush])

  const handleChange = (next: string) => {
    setValue(next)
    pendingName.current = next
    clearTimeout(timer.current)
    timer.current = setTimeout(flush, RENAME_DEBOUNCE_MS)
  }

  const handleBlur = () => {
    setFocused(false)
    // No rename of ours pending: adopt any rename that arrived while focused.
    if (pendingName.current === null) setValue(entity.name)
    flush()
  }

  const isDefaultName = entity.name === `Untitled ${structureName}`
  const RENAME_DEBOUNCE_MS = 300

  return (
    <Input
      value={value}
      autoFocus={isDefaultName}
      onFocus={() => setFocused(true)}
      onBlur={handleBlur}
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
  const structure = useStructure(entity.structureType)
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

    switch (def.kind) {
      case PropertyKind.RICHTEXT: {
        if (value?.case !== 'richtext') return null
        return <EntityRichTextField key={def.id} propertyId={def.id} propertyRef={value.value} />
      }
      case PropertyKind.DATE: {
        const iso = value?.case === 'date' ? value.value : undefined
        return (
          <EntityDateField
            key={def.id}
            label={def.label || 'Date'}
            iso={iso}
            onChange={(next) => setProperty(def.id, { case: 'date', value: next })}
            onClear={iso ? () => setProperty(def.id, null) : undefined}
          />
        )
      }
      case PropertyKind.SELECT: {
        if (def.options.length === 0) return null
        const current = value?.case === 'select' ? value.value : def.defaultOption
        return (
          <EntitySelectField
            key={def.id}
            label={def.label || def.id}
            value={current}
            options={def.options}
            onChange={(next) => setProperty(def.id, { case: 'select', value: next })}
          />
        )
      }
      case PropertyKind.RELATIONS: {
        if (!def.targetStructure) return null
        const refs = value?.case === 'relations' ? value.value.refs : []
        return (
          <EntityRelationsField
            key={def.id}
            label={def.label || def.id}
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
