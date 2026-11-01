import { useEffect, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { ArrowBendDownRightIcon, CaretRightIcon, GraphIcon, PlusIcon, XIcon } from '@phosphor-icons/react'
import { EmptyLine } from '~/components/empty-state/EmptyLine'
import { Button } from '~/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '~/components/ui/collapsible'
import { EntityPicker } from '~/components/entity/EntityPicker'
import {
  RELATED_SCOPE,
  useAddRelationship,
  useRemoveRelationship,
  type Entity,
  type RelationTarget,
} from '~/model/store'
import { structurePresentation, useRelatesTo } from '~/model/structures'
import { useRelationshipRows, type RelationshipReason, type RelationshipRow } from '~/model/relationships'

interface RelationshipsPanelProps {
  entity: Entity
}

// An entity's relationships section (ADR 12): the entities it relates to or mentions and
// the ones that relate to, mention or name it in a relation property.
export function RelationshipsPanel({ entity }: RelationshipsPanelProps) {
  const rows = useRelationshipRows(entity.id)
  const [open, setOpen] = useState(false)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === 'B' || e.key === 'b')) {
        e.preventDefault()
        setOpen((o) => !o)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="flex items-center gap-2">
        <CollapsibleTrigger className="group flex flex-1 items-center gap-2 text-left text-sm font-medium text-muted-foreground hover:text-foreground">
          <CaretRightIcon className="size-3.5 transition-transform group-data-[state=open]:rotate-90" />
          <span>Relationships ({rows.length})</span>
        </CollapsibleTrigger>
        <AddRelationship entity={entity} onAdded={() => setOpen(true)} />
      </div>
      <CollapsibleContent className="pt-3">
        {open && <RelationshipsList key={entity.id} entity={entity} rows={rows} />}
      </CollapsibleContent>
    </Collapsible>
  )
}

interface AddRelationshipProps {
  entity: Entity
  // Called after a pick, so the section can open and show the new row.
  onAdded: () => void
}

// The "Add" button and its picker, limited to the structures `entity` may relate to.
function AddRelationship({ entity, onAdded }: AddRelationshipProps) {
  const relatesTo = useRelatesTo(entity.structureType)
  const addRelationship = useAddRelationship()
  if (relatesTo.length === 0) return null

  const related = entity.links.filter((l) => l.sourcePropertyId === RELATED_SCOPE).map((l) => l.target?.id ?? '')
  const excludeIds = [entity.id, ...related]

  const handlePick = (target: RelationTarget) => {
    addRelationship(entity, target)
    onAdded()
  }

  return (
    <EntityPicker targetStructures={relatesTo} excludeIds={excludeIds} onPick={handlePick}>
      <Button variant="ghost" size="sm" className="h-6 gap-1 px-1.5 text-xs font-normal text-muted-foreground">
        <PlusIcon className="size-3" />
        Add
      </Button>
    </EntityPicker>
  )
}

interface RelationshipsListProps {
  entity: Entity
  rows: RelationshipRow[]
}

function RelationshipsList({ entity, rows }: RelationshipsListProps) {
  const [showAll, setShowAll] = useState(false)
  const removeRelationship = useRemoveRelationship()
  if (rows.length === 0) {
    return (
      <EmptyLine icon={GraphIcon} color="var(--chart-1)" className="px-0">
        No relationships yet. Mention a note with @, or add one.
      </EmptyLine>
    )
  }

  // After this many rows the rest wait behind "Show all", so a hub note stays readable.
  const collapsedRowCount = 20
  const visible = showAll ? rows : rows.slice(0, collapsedRowCount)

  return (
    <div className="flex flex-col gap-1">
      <ul className="flex flex-col gap-0.5">
        {visible.map((row) => (
          <RelationshipRowItem
            key={`${row.direction}:${row.entityId}`}
            row={row}
            onRemove={() => removeRelationship(entity, row.entityId)}
          />
        ))}
      </ul>
      {visible.length < rows.length && (
        <Button variant="ghost" size="sm" className="self-start text-muted-foreground" onClick={() => setShowAll(true)}>
          Show all ({rows.length})
        </Button>
      )}
    </div>
  )
}

interface RelationshipRowItemProps {
  row: RelationshipRow
  onRemove: () => void
}

function RelationshipRowItem({ row, onRemove }: RelationshipRowItemProps) {
  const { icon: Icon, color } = structurePresentation(row.structureType)

  const outgoing = row.direction === 'outgoing'
  const related = row.reasons.some((r) => r.kind === 'related')
  const mentioned = row.reasons.some((r) => r.kind === 'mentioned')
  const labels = row.reasons.map((reason) => reasonLabel(reason, row.direction)).join(' · ')
  const controlClassName = 'rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground'

  // Only a direct relationship can be removed here. A mention is the author's text, so a
  // mention-only row jumps to it instead.
  const renderControl = () => {
    if (!outgoing) return null
    if (related) {
      return (
        <button
          type="button"
          onClick={onRemove}
          className={controlClassName}
          aria-label={`Remove relationship to ${row.name}`}
          title="Remove relationship"
        >
          <XIcon className="size-3.5" />
        </button>
      )
    }
    if (mentioned) {
      return (
        <button
          type="button"
          onClick={() => jumpToMention(row.entityId)}
          className={controlClassName}
          aria-label={`Go to mention of ${row.name}`}
          title="Go to mention"
        >
          <ArrowBendDownRightIcon className="size-3.5" />
        </button>
      )
    }
    return null
  }

  return (
    <li className="flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-muted">
      <Link to="/e/$id" params={{ id: row.entityId }} className="flex min-w-0 flex-1 items-center gap-2">
        <Icon className="size-4 shrink-0" style={{ color }} aria-hidden />
        <span className="truncate">{row.name}</span>
      </Link>
      <span className="shrink-0 text-xs text-muted-foreground">{labels}</span>
      {renderControl()}
    </li>
  )
}

// How a row reads from the entity's side: a link it holds is "related" or "mentioned"; a
// link held by the other entity is "related to this" or "mentions this".
function reasonLabel(reason: RelationshipReason, direction: RelationshipRow['direction']): string {
  switch (reason.kind) {
    case 'related':
      return direction === 'outgoing' ? 'related' : 'related to this'
    case 'mentioned':
      return direction === 'outgoing' ? 'mentioned' : 'mentions this'
    case 'property':
      return `via ${reason.label}`
  }
}

// Scrolls the first chip for `entityId` on the page into view and rings it for a second.
// The chips are in document order, so with several it's the first one in the first
// document, and the entity's documents are in registry order.
function jumpToMention(entityId: string) {
  const ring = ['ring-2', 'ring-primary']
  const chips = document.querySelectorAll<HTMLElement>('main [data-type="mention"], main [data-type="hashtag"]')
  const chip = Array.from(chips).find((el) => el.dataset.id === entityId)
  if (!chip) return
  chip.scrollIntoView({ block: 'center' })
  chip.classList.add(...ring)
  window.setTimeout(() => chip.classList.remove(...ring), 1000)
}
