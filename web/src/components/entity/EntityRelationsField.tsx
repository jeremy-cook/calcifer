import { Link } from '@tanstack/react-router'
import { PlusIcon, XIcon } from '@phosphor-icons/react'
import type { EntityRef } from 'calcifer/v1/entities_pb'
import { EntityPicker } from '~/components/entity/EntityPicker'
import { PILL_CLASS, PILL_CLEAR_CLASS, PILL_EMPTY_CLASS } from '~/components/entity/property-pill'
import { useEntitiesByIds, type RelationTarget } from '~/model/store'
import { structurePresentation, useStructure } from '~/model/structures'

export interface EntityRelationsFieldProps {
  // The entity being edited, left out of the picker's candidates.
  selfId: string
  label: string
  // Empty: any structure.
  targetStructure: string
  refs: readonly EntityRef[]
  onAdd: (target: RelationTarget) => void
  onRemove: (targetId: string) => void
}

// One pill per target, tinted by its structure, then a "+ Tag" pill to add one.
export function EntityRelationsField({
  selfId,
  label,
  targetStructure,
  refs,
  onAdd,
  onRemove,
}: EntityRelationsFieldProps) {
  const entities = useEntitiesByIds(refs.map((ref) => ref.id))
  const target = useStructure(targetStructure)

  const resolvedRefs = refs
    .map((ref, i) => ({ ref, entity: entities[i] }))
    .filter((r): r is { ref: EntityRef; entity: NonNullable<typeof r.entity> } => r.entity !== undefined)

  const excludeIds = [selfId, ...refs.map((r) => r.id)]
  const addLabel = target?.name ?? label

  return (
    <>
      {resolvedRefs.map(({ ref, entity }) => {
        const { icon: Icon, color } = structurePresentation(entity.structureType)
        return (
          <span key={ref.id} className={PILL_CLASS} style={{ color }}>
            <Link to="/e/$id" params={{ id: entity.id }} className="inline-flex items-center gap-1 hover:underline">
              <Icon weight="duotone" aria-hidden />
              {entity.name}
            </Link>
            <button
              type="button"
              onClick={() => onRemove(ref.id)}
              aria-label={`Remove ${entity.name}`}
              className={PILL_CLEAR_CLASS}
            >
              <XIcon />
            </button>
          </span>
        )
      })}
      <EntityPicker targetStructure={targetStructure} excludeIds={excludeIds} onPick={onAdd}>
        <button type="button" className={PILL_EMPTY_CLASS} aria-label={`Add to ${label.toLowerCase()}`}>
          <PlusIcon />
          {addLabel}
        </button>
      </EntityPicker>
    </>
  )
}
