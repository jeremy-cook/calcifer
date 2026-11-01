import { PlusIcon, XIcon } from '@phosphor-icons/react'
import type { EntityRef } from 'calcifer/v1/entities_pb'
import { EntityPicker } from '~/components/entity/EntityPicker'
import { PILL_CLASS, PILL_CLEAR_CLASS, PILL_EMPTY_CLASS } from '~/components/entity/property-pill'
import { useEntity, type RelationTarget } from '~/model/store'

export interface EntityRelationFieldProps {
  // The entity being edited, left out of the picker's candidates.
  selfId: string
  label: string
  // The empty pill's words, when a label shows above it already (a facet's band): "Set".
  emptyLabel?: string
  // Empty: any structure.
  targetStructure: string
  target?: EntityRef
  onChange: (target: RelationTarget | null) => void
}

// A single-target relation: the target's pill opens the picker to replace it.
export function EntityRelationField({
  selfId,
  label,
  emptyLabel,
  targetStructure,
  target,
  onChange,
}: EntityRelationFieldProps) {
  // A target with no entity (deleted) shows as unset.
  const { entity: targetEntity } = useEntity(target?.id ?? '')
  const excludeIds = target ? [selfId, target.id] : [selfId]

  if (!targetEntity) {
    return (
      <EntityPicker targetStructure={targetStructure} excludeIds={excludeIds} onPick={onChange}>
        <button type="button" className={PILL_EMPTY_CLASS} aria-label={`Set ${label.toLowerCase()}`}>
          <PlusIcon />
          {emptyLabel ?? label}
        </button>
      </EntityPicker>
    )
  }

  return (
    <span className={PILL_CLASS}>
      <EntityPicker targetStructure={targetStructure} excludeIds={excludeIds} onPick={onChange}>
        <button type="button" aria-label={`${label}: ${targetEntity.name}`}>
          <span className="font-medium text-muted-foreground">{label}</span> {targetEntity.name}
        </button>
      </EntityPicker>
      <button
        type="button"
        onClick={() => onChange(null)}
        aria-label={`Clear ${label.toLowerCase()}`}
        className={PILL_CLEAR_CLASS}
      >
        <XIcon />
      </button>
    </span>
  )
}
