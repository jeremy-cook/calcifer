import { PlusIcon, XIcon } from '@phosphor-icons/react'
import type { EntityRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { Badge } from '~/components/ui/badge'
import { Button } from '~/components/ui/button'
import { EntityPicker } from '~/components/entity/EntityPicker'
import { useAllEntities, type RelationTarget } from '~/model/store'

export interface EntityRelationsFieldProps {
  // The entity being edited, left out of the picker's candidates.
  selfId: string
  label: string
  // Empty: any structure.
  targetStructure: string
  refs: readonly EntityRef[]
  onChange: (refs: RelationTarget[]) => void
}

export function EntityRelationsField({ selfId, label, targetStructure, refs, onChange }: EntityRelationsFieldProps) {
  const entities = useAllEntities()

  const resolvedRefs = refs
    .map((ref) => ({ ref, entity: entities.find((e) => e.id === ref.id) }))
    .filter((r): r is { ref: EntityRef; entity: NonNullable<typeof r.entity> } => r.entity !== undefined)

  const removeRef = (id: string) => {
    onChange(refs.filter((r) => r.id !== id))
  }

  const addRef = (target: RelationTarget) => {
    onChange([...refs, target])
  }

  const excludeIds = [selfId, ...refs.map((r) => r.id)]

  return (
    <div className="flex items-start gap-3 px-12 py-3">
      <span className="w-24 shrink-0 pt-1 text-sm text-muted-foreground">{label}</span>
      <div className="flex flex-1 flex-wrap items-center gap-1.5">
        {resolvedRefs.map(({ ref, entity }) => (
          <Badge key={ref.id} variant="secondary" className="gap-1">
            {entity.name}
            <button
              type="button"
              onClick={() => removeRef(ref.id)}
              aria-label={`Remove ${entity.name}`}
              className="rounded-full hover:text-destructive"
            >
              <XIcon className="size-3" />
            </button>
          </Badge>
        ))}
        <EntityPicker targetStructure={targetStructure} excludeIds={excludeIds} onPick={addRef}>
          <Button variant="ghost" size="sm" className="h-6 gap-1 px-1.5 text-xs font-normal text-muted-foreground">
            <PlusIcon className="size-3" />
            Add
          </Button>
        </EntityPicker>
      </div>
    </div>
  )
}
