import { useCallback, useEffect, useRef, useState } from 'react'
import { createFileRoute, useLocation, useNavigate } from '@tanstack/react-router'
import { CheckSquareIcon, TrashIcon } from '@phosphor-icons/react'
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
import { EntityRelationField } from '~/components/entity/EntityRelationField'
import { EntityRelationsField } from '~/components/entity/EntityRelationsField'
import { EntityTextField } from '~/components/entity/EntityTextField'
import { TodoStatusField } from '~/components/todo/TodoStatusField'
import { DailyNoteDateField } from '~/components/calendar/DailyNoteDateField'
import { RelationshipsPanel } from '~/components/relationships/RelationshipsPanel'
import {
  deleteEntity,
  hasFacet,
  propertyValueOf,
  useAttachFacet,
  useDetachFacet,
  useEntity,
  usePropertyWriters,
  useRenameEntity,
  type Entity,
} from '~/model/store'
import {
  PropertyKind,
  isNameEditable,
  useFacets,
  useStructure,
  type FacetDef,
  type PropertyDef,
} from '~/model/structures'
import { TODO_FACET } from '~/model/todos'

export const Route = createFileRoute('/e/$id')({
  component: function EntityRoute() {
    const { id } = Route.useParams()
    const { entity, status } = useEntity(id)
    if (status === 'loading') return null
    if (!entity) return <EntityNotFound />

    return (
      <div className="flex h-full flex-col">
        <EntityHeader entity={entity} />
        <div className="min-h-0 flex-1 overflow-y-auto">
          <EntityProperties entity={entity} />
          <div className="border-t border-border px-12 py-6">
            <RelationshipsPanel entity={entity} />
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
  const navigate = useNavigate()
  const attachFacet = useAttachFacet()
  const structure = useStructure(entity.structureType)

  const structureName = structure?.name ?? entity.structureType
  const nameEditable = isNameEditable(entity.structureType)
  const canMakeTodo = (structure?.facets.includes(TODO_FACET) ?? false) && !hasFacet(entity, TODO_FACET)

  const handleDelete = () => {
    deleteEntity(entity.id)
    void navigate({ to: '/' })
  }

  const renderTitle = () =>
    nameEditable ? (
      <EntityTitleInput key={entity.id} entity={entity} />
    ) : (
      <h1 className="flex h-12 flex-1 items-center px-3 font-heading text-3xl font-semibold text-heading">
        {entity.name}
      </h1>
    )

  return (
    <div className="flex items-center gap-2 border-b border-border px-12 py-6">
      {renderTitle()}
      {canMakeTodo && (
        <Button variant="ghost" size="sm" onClick={() => void attachFacet(entity.id, TODO_FACET)}>
          <CheckSquareIcon />
          Make a ToDo
        </Button>
      )}
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
}

// Renames are debounced so a typing burst sends one Rename. While the field is
// focused, `value` is the source of truth and entity.name is never synced into
// it: watch-stream echoes of earlier writes can land after a later optimistic
// update and briefly move entity.name back to an older prefix. Outside of focus,
// external renames (another client, the MCP agent) are adopted during render.
// The title is focused only on arrival from a create flow, which marks the
// navigation with `justCreated` history state.
function EntityTitleInput({ entity }: EntityTitleInputProps) {
  const renameEntity = useRenameEntity()
  const justCreated = useLocation({ select: (location) => location.state.justCreated === true })
  const [value, setValue] = useState(entity.name)
  const [focused, setFocused] = useState(false)
  const [syncedName, setSyncedName] = useState(entity.name)
  const pendingName = useRef<string | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const entityId = entity.id

  if (entity.name !== syncedName) {
    setSyncedName(entity.name)
    if (!focused) setValue(entity.name)
  }

  const flush = useCallback(() => {
    clearTimeout(timer.current)
    timer.current = undefined
    const name = pendingName.current
    if (name === null) return
    pendingName.current = null
    renameEntity(entityId, name)
  }, [renameEntity, entityId])

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

  const RENAME_DEBOUNCE_MS = 300

  return (
    <Input
      value={value}
      autoFocus={justCreated}
      onFocus={() => setFocused(true)}
      onBlur={handleBlur}
      onChange={(e) => handleChange(e.target.value)}
      className="h-12 border-0 bg-transparent font-heading text-3xl font-semibold text-heading shadow-none focus-visible:ring-0 md:text-3xl dark:bg-transparent"
      aria-label="Entity title"
    />
  )
}

interface EntityPropertiesProps {
  entity: Entity
}

function EntityProperties({ entity }: EntityPropertiesProps) {
  const { setText, setNumber, setDate, setSelect, setRelation, addRelationTarget, removeRelationTarget } =
    usePropertyWriters()
  const structure = useStructure(entity.structureType)
  const facets = useFacets().filter((f) => hasFacet(entity, f.id))

  // Properties whose editor isn't the generic one for their type. Structure
  // properties are keyed "Structure.propertyId"; facet properties by their full
  // id ("todo.status"), which already carries the facet.
  const overrides: Record<string, () => React.ReactNode> = {
    'DailyNote.date': () => <DailyNoteDateField key="date" entity={entity} />,
    'todo.status': () => <TodoStatusField key="todo.status" entity={entity} />,
  }

  const renderProperty = (def: PropertyDef, facet?: FacetDef) => {
    const override = overrides[facet ? def.id : `${entity.structureType}.${def.id}`]
    if (override) return override()

    const value = propertyValueOf(entity, def.id)

    switch (def.kind) {
      case PropertyKind.RICHTEXT:
        return <EntityRichTextField key={def.id} entityId={entity.id} propertyId={def.id} />
      case PropertyKind.TEXT:
        return (
          <EntityTextField
            key={def.id}
            label={def.label || def.id}
            value={value?.case === 'text' ? value.value : ''}
            onCommit={(draft) => setText(entity, def.id, draft === '' ? null : draft)}
          />
        )
      case PropertyKind.NUMBER: {
        const current = value?.case === 'number' ? value.value : undefined
        // An empty (or, from a number input, unparseable) draft clears the property.
        const commitNumber = (draft: string) => {
          const next = draft.trim() === '' ? null : Number(draft)
          if ((next ?? undefined) === current || (next !== null && !Number.isFinite(next))) return
          setNumber(entity, def.id, next)
        }
        return (
          <EntityTextField
            key={def.id}
            type="number"
            label={def.label || def.id}
            value={current === undefined ? '' : String(current)}
            onCommit={commitNumber}
          />
        )
      }
      case PropertyKind.DATE: {
        const iso = value?.case === 'date' ? value.value : undefined
        return (
          <EntityDateField
            key={def.id}
            label={def.label || 'Date'}
            iso={iso}
            onChange={(next) => setDate(entity, def.id, next)}
            onClear={iso ? () => setDate(entity, def.id, null) : undefined}
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
            onChange={(next) => setSelect(entity, def.id, next)}
          />
        )
      }
      case PropertyKind.RELATIONS: {
        const refs = value?.case === 'relations' ? value.value.refs : []
        return (
          <EntityRelationsField
            key={def.id}
            selfId={entity.id}
            label={def.label || def.id}
            targetStructure={def.targetStructure}
            refs={refs}
            onAdd={(target) => addRelationTarget(entity, def.id, target)}
            onRemove={(targetId) => removeRelationTarget(entity, def.id, targetId)}
          />
        )
      }
      case PropertyKind.RELATION:
        return (
          <EntityRelationField
            key={def.id}
            selfId={entity.id}
            label={def.label || def.id}
            targetStructure={def.targetStructure}
            target={value?.case === 'relation' ? value.value : undefined}
            onChange={(target) => setRelation(entity, def.id, target)}
          />
        )
      // UNSPECIFIED, or a kind added to the server after this build.
      default:
        return null
    }
  }

  return (
    <>
      {structure?.properties.map((def) => renderProperty(def))}
      {facets.map((facet) => (
        <FacetSection key={facet.id} entity={entity} facet={facet} renderProperty={renderProperty} />
      ))}
    </>
  )
}

interface FacetSectionProps {
  entity: Entity
  facet: FacetDef
  renderProperty: (def: PropertyDef, facet: FacetDef) => React.ReactNode
}

function FacetSection({ entity, facet, renderProperty }: FacetSectionProps) {
  const detachFacet = useDetachFacet()

  return (
    <section className="border-t border-border py-3">
      <div className="flex items-center justify-between px-12 py-1">
        <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{facet.name}</h2>
        <Button variant="ghost" size="sm" onClick={() => void detachFacet(entity.id, facet.id)}>
          Remove
        </Button>
      </div>
      {facet.properties.map((def) => renderProperty(def, facet))}
    </section>
  )
}
