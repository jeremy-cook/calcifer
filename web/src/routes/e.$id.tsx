import { useCallback, useEffect, useRef, useState } from 'react'
import { createFileRoute, Link, useLocation, useNavigate } from '@tanstack/react-router'
import { formatDistanceToNow } from 'date-fns'
import { CaretDownIcon, DotsThreeIcon, FileDashedIcon, TrashIcon } from '@phosphor-icons/react'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '~/components/ui/alert-dialog'
import { Button } from '~/components/ui/button'
import { LoadingText } from '~/components/loading/LoadingText'
import { useShownAfterDelay } from '~/components/loading/use-shown-after'
import { EmptyState } from '~/components/empty-state/EmptyState'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '~/components/ui/dropdown-menu'
import { Input } from '~/components/ui/input'
import { EntityRichTextField } from '~/components/entity/EntityRichTextField'
import { EntityDateField } from '~/components/entity/EntityDateField'
import { EntitySelectField } from '~/components/entity/EntitySelectField'
import { EntityRelationField } from '~/components/entity/EntityRelationField'
import { EntityRelationsField } from '~/components/entity/EntityRelationsField'
import { EntityTextField } from '~/components/entity/EntityTextField'
import { PILL_CLASS } from '~/components/entity/property-pill'
import { TodoPriorityField } from '~/components/todo/TodoPriorityField'
import { TodoStatusField } from '~/components/todo/TodoStatusField'
import { DailyNoteDateField } from '~/components/calendar/DailyNoteDateField'
import { ReferencesList } from '~/components/relationships/ReferencesList'
import { RelationshipsPanel } from '~/components/relationships/RelationshipsPanel'
import { cn } from '~/lib/utils'
import { entityTitle } from '~/model/daily-notes'
import { formatShortDate, weekNumber } from '~/model/dates'
import { useIncomingReferences } from '~/model/relationships'
import {
  dailyNoteDate,
  deleteEntity,
  entityUpdatedAtDate,
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
  facetPresentation,
  isNameEditable,
  optionLabel,
  richTextPropertyIds,
  structurePresentation,
  useFacets,
  useStructure,
  type FacetDef,
  type PropertyDef,
} from '~/model/structures'

export const Route = createFileRoute('/e/$id')({
  component: function EntityRoute() {
    const { id } = Route.useParams()
    const { entity, status } = useEntity(id)
    if (status === 'loading') return <EntityLoading />
    if (!entity) return <EntityNotFound />

    return (
      <div className="flex h-full flex-col">
        <EntityHeader key={entity.id} entity={entity} />
        <div className="min-h-0 flex-1 overflow-y-auto">
          <EntityBody entity={entity} />
        </div>
      </div>
    )
  },
})

// The note page's shape, faint, until the first snapshot says whether the note exists.
function EntityLoading() {
  const shown = useShownAfterDelay()
  if (!shown) return null

  return (
    <div className="flex flex-col">
      <header className="flex flex-col gap-3 px-7 @3xl:px-16 pt-8 pb-5">
        <div className="loading-bar h-3 w-40" />
        <div className="loading-bar h-8 w-80" />
      </header>
      <LoadingText className="px-7 @3xl:px-16 py-10" />
    </div>
  )
}

function EntityNotFound() {
  return (
    <div className="px-7 @3xl:px-16 py-10">
      <EmptyState
        icon={FileDashedIcon}
        color="var(--muted-foreground)"
        title="Gone up the chimney"
        description="This note doesn't exist, or was deleted."
        action={
          <Button variant="outline" asChild>
            <Link to="/">Go home</Link>
          </Button>
        }
      />
    </div>
  )
}

interface EntityHeaderProps {
  entity: Entity
}

// The byline (kind, when edited, the ⋯ menu), the title, then the properties as a row
// of pills: the structure's own, then one summary pill per attached facet. A facet's
// pill opens its fields in a band under the header.
function EntityHeader({ entity }: EntityHeaderProps) {
  const [openFacetId, setOpenFacetId] = useState<string | null>(null)
  const openFacet = useFacets().find((f) => f.id === openFacetId && hasFacet(entity, f.id))

  const toggleFacet = (facetId: string) => setOpenFacetId((open) => (open === facetId ? null : facetId))

  const renderTitle = () =>
    isNameEditable(entity.structureType) ? (
      <EntityTitleInput key={entity.id} entity={entity} />
    ) : (
      <h1 className="font-heading text-4xl leading-tight font-semibold text-heading">{entityTitle(entity)}</h1>
    )

  return (
    <div>
      <header className="flex flex-col gap-1.5 px-7 @3xl:px-16 pt-7 pb-5">
        <EntityByline entity={entity} onFacetAttached={setOpenFacetId} />
        {renderTitle()}
        <EntityPills entity={entity} openFacetId={openFacet?.id ?? null} onToggleFacet={toggleFacet} />
      </header>
      {openFacet && <FacetBand entity={entity} facet={openFacet} />}
    </div>
  )
}

interface EntityBylineProps {
  entity: Entity
  onFacetAttached: (facetId: string) => void
}

function EntityByline({ entity, onFacetAttached }: EntityBylineProps) {
  const structure = useStructure(entity.structureType)
  const { icon: Icon, color } = structurePresentation(entity.structureType)

  const dateIso = dailyNoteDate(entity)
  // An entity with no text of its own is about what refers to it, so the byline counts them.
  const referenceCount = useIncomingReferences(entity.id).length
  const hasText = richTextPropertyIds(entity.structureType).length > 0
  const details = [
    dateIso ? `Week ${weekNumber(dateIso)}` : null,
    hasText ? null : `${referenceCount} ${referenceCount === 1 ? 'note' : 'notes'}`,
    `edited ${formatDistanceToNow(entityUpdatedAtDate(entity), { addSuffix: true })}`,
  ].filter((detail) => detail !== null)

  return (
    <div className="flex h-7 items-center gap-2 text-xs font-bold tracking-wider text-muted-foreground uppercase">
      <Icon weight="duotone" className="size-4.5 shrink-0" style={{ color }} aria-hidden />
      <span className="shrink-0">{structure?.name ?? entity.structureType}</span>
      <span className="min-w-0 truncate font-medium tracking-normal normal-case">· {details.join(' · ')}</span>
      <EntityMenu entity={entity} onFacetAttached={onFacetAttached} />
    </div>
  )
}

interface EntityMenuProps {
  entity: Entity
  // Called after a facet is attached, so its band can open.
  onFacetAttached: (facetId: string) => void
}

// Attaching a facet the structure allows, and deleting the entity behind a confirmation.
function EntityMenu({ entity, onFacetAttached }: EntityMenuProps) {
  const navigate = useNavigate()
  const attachFacet = useAttachFacet()
  const structure = useStructure(entity.structureType)
  const facets = useFacets().filter((f) => structure?.facets.includes(f.id) && !hasFacet(entity, f.id))
  const [confirmOpen, setConfirmOpen] = useState(false)

  const structureName = structure?.name ?? entity.structureType

  const handleAttach = (facetId: string) => {
    void attachFacet(entity.id, facetId)
    onFacetAttached(facetId)
  }

  const handleDelete = () => {
    deleteEntity(entity.id)
    void navigate({ to: '/' })
  }

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" className="ml-auto text-muted-foreground" aria-label="More actions">
            <DotsThreeIcon weight="bold" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48">
          {facets.map((facet) => {
            const { icon: FacetIcon, color } = facetPresentation(facet.id)
            return (
              <DropdownMenuItem key={facet.id} onSelect={() => handleAttach(facet.id)}>
                <FacetIcon weight="duotone" style={{ color }} />
                Make a {facet.name}
              </DropdownMenuItem>
            )
          })}
          {facets.length > 0 && <DropdownMenuSeparator />}
          <DropdownMenuItem variant="destructive" onSelect={() => setConfirmOpen(true)}>
            <TrashIcon />
            Delete {structureName.toLowerCase()}…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {structureName.toLowerCase()}?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{entity.name}" and its content.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={handleDelete}>
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
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
      className="h-auto rounded-none border-0 bg-transparent p-0 font-heading text-4xl leading-tight font-semibold text-heading shadow-none focus-visible:ring-0 md:text-4xl dark:bg-transparent"
      aria-label="Entity title"
    />
  )
}

// Renders a property's editor as a pill. Facet properties render in the facet's band,
// under their own label, so a date there leaves its label out of the pill.
function usePropertyRenderer(entity: Entity) {
  const { setText, setNumber, setDate, setSelect, setRelation, addRelationTarget, removeRelationTarget } =
    usePropertyWriters()

  // Properties whose editor isn't the generic one for their type. Structure
  // properties are keyed "Structure.propertyId"; facet properties by their full
  // id ("todo.status"), which already carries the facet.
  const overrides: Record<string, () => React.ReactNode> = {
    'DailyNote.date': () => <DailyNoteDateField key="date" entity={entity} />,
    'todo.status': () => <TodoStatusField key="todo.status" entity={entity} />,
    'todo.priority': () => <TodoPriorityField key="todo.priority" entity={entity} />,
  }

  const renderProperty = (def: PropertyDef, facet?: FacetDef) => {
    const override = overrides[facet ? def.id : `${entity.structureType}.${def.id}`]
    if (override) return override()

    const value = propertyValueOf(entity, def.id)

    switch (def.kind) {
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
            showLabel={!facet}
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
      // RICHTEXT renders in the body; UNSPECIFIED, or a kind added to the server after this build.
      default:
        return null
    }
  }

  return renderProperty
}

interface EntityPillsProps {
  entity: Entity
  openFacetId: string | null
  onToggleFacet: (facetId: string) => void
}

// The structure's properties, then a summary pill per attached facet after a divider.
function EntityPills({ entity, openFacetId, onToggleFacet }: EntityPillsProps) {
  const renderProperty = usePropertyRenderer(entity)
  const structure = useStructure(entity.structureType)
  const attached = useFacets().filter((f) => hasFacet(entity, f.id))

  const structureProperties = structure?.properties.filter((def) => def.kind !== PropertyKind.RICHTEXT) ?? []

  const groups = [
    structureProperties.length > 0 && (
      <PillGroup key="structure">{structureProperties.map((def) => renderProperty(def))}</PillGroup>
    ),
    attached.length > 0 && (
      <PillGroup key="facets">
        {attached.map((facet) => (
          <FacetPill
            key={facet.id}
            entity={entity}
            facet={facet}
            open={facet.id === openFacetId}
            onToggle={() => onToggleFacet(facet.id)}
          />
        ))}
      </PillGroup>
    ),
  ].filter(Boolean)

  if (groups.length === 0) return null

  return <div className="mt-2 flex flex-wrap items-center gap-1.5">{groups}</div>
}

interface PillGroupProps {
  children: React.ReactNode
}

// Pills that belong together; a divider sets each group after the first apart.
function PillGroup({ children }: PillGroupProps) {
  return (
    <span className="contents [&+&]:before:mx-1.5 [&+&]:before:h-4 [&+&]:before:w-px [&+&]:before:bg-muted-foreground/40 [&+&]:before:content-['']">
      {children}
    </span>
  )
}

interface FacetPillProps {
  entity: Entity
  facet: FacetDef
  open: boolean
  onToggle: () => void
}

// A facet's name and the values it has set ("ToDo · High · Due Fri 6 Nov"); it opens
// and closes the facet's band.
function FacetPill({ entity, facet, open, onToggle }: FacetPillProps) {
  const { icon: Icon, color } = facetPresentation(facet.id)

  // Set values only: a select left on its default and an empty field are skipped.
  const summary = facet.properties.flatMap((def) => {
    const value = propertyValueOf(entity, def.id)
    switch (value?.case) {
      case 'select':
        return value.value === def.defaultOption ? [] : [optionLabel(def, value.value)]
      case 'date':
        return [`${def.label || 'Date'} ${formatShortDate(value.value)}`]
      case 'text':
        return value.value ? [value.value] : []
      case 'number':
        return [String(value.value)]
      default:
        return []
    }
  })

  return (
    <button
      type="button"
      onClick={onToggle}
      aria-expanded={open}
      className={cn(PILL_CLASS, open && 'bg-accent text-accent-foreground ring-1 ring-border')}
    >
      <Icon weight="duotone" style={{ color }} aria-hidden />
      {facet.name}
      {summary.map((part) => (
        <span key={part} className="font-medium">
          <span className="text-muted-foreground">·</span> {part}
        </span>
      ))}
      <CaretDownIcon className={cn('size-3! text-muted-foreground transition-transform', open && 'rotate-180')} />
    </button>
  )
}

interface FacetBandProps {
  entity: Entity
  facet: FacetDef
}

// An open facet's fields, in a band the full width of the page under the header; the
// editor's top border closes it.
function FacetBand({ entity, facet }: FacetBandProps) {
  const renderProperty = usePropertyRenderer(entity)
  const detachFacet = useDetachFacet()
  const { icon: Icon, color } = facetPresentation(facet.id)

  return (
    <section aria-label={facet.name} className="border-t border-border bg-muted/60 px-7 @3xl:px-16 pt-2.5 pb-3.5">
      <div className="flex h-7 items-center gap-2 text-xs font-bold tracking-wider uppercase" style={{ color }}>
        <Icon weight="duotone" className="size-4" aria-hidden />
        {facet.name}
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-7 font-medium tracking-normal text-muted-foreground normal-case"
          onClick={() => void detachFacet(entity.id, facet.id)}
          aria-label={`Remove ${facet.name}`}
        >
          Remove
        </Button>
      </div>
      <div className="mt-1 flex flex-wrap gap-x-10 gap-y-2">
        {facet.properties.map((def) => (
          <div key={def.id} className="flex min-w-36 flex-col items-start gap-1">
            <span className="text-xs text-muted-foreground">{def.label || def.id}</span>
            {renderProperty(def, facet)}
          </div>
        ))}
      </div>
    </section>
  )
}

interface EntityBodyProps {
  entity: Entity
}

// The structure's rich-text properties and the relationships section below them. An
// entity with no text of its own (a tag) shows what refers to it instead.
function EntityBody({ entity }: EntityBodyProps) {
  const structure = useStructure(entity.structureType)
  const richText = structure?.properties.filter((def) => def.kind === PropertyKind.RICHTEXT) ?? []

  if (richText.length === 0) {
    return (
      <div className="px-7 @3xl:px-16 pb-10">
        <ReferencesList entity={entity} />
      </div>
    )
  }

  return (
    <>
      {richText.map((def) => (
        <EntityRichTextField key={def.id} entityId={entity.id} propertyId={def.id} />
      ))}
      <div className="border-t border-border px-7 @3xl:px-16 py-6">
        <RelationshipsPanel entity={entity} />
      </div>
    </>
  )
}
