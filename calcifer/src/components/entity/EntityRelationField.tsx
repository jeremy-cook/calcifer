import { XIcon } from '@phosphor-icons/react'
import type { EntityRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { Button } from '~/components/ui/button'
import { EntityPicker } from '~/components/entity/EntityPicker'
import { useAllEntities, type RelationTarget } from '~/model/store'

export interface EntityRelationFieldProps {
  label: string
  // Empty: any structure.
  targetStructure: string
  target?: EntityRef
  onChange: (target: RelationTarget | null) => void
}

// A single-target relation: the target's name opens the picker to replace it.
export function EntityRelationField({ label, targetStructure, target, onChange }: EntityRelationFieldProps) {
  const entities = useAllEntities()
  // A target with no entity (deleted) shows as unset.
  const targetEntity = target && entities.find((e) => e.id === target.id)

  return (
    <div className="flex items-center gap-3 px-12 py-3">
      <span className="w-24 shrink-0 text-sm text-muted-foreground">{label}</span>
      <EntityPicker targetStructure={targetStructure} excludeIds={target ? [target.id] : []} onPick={onChange}>
        <Button variant="ghost" size="sm" className="h-8 px-2 font-normal">
          {targetEntity ? targetEntity.name : <span className="text-muted-foreground">Set {label.toLowerCase()}</span>}
        </Button>
      </EntityPicker>
      {target && (
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          onClick={() => onChange(null)}
          aria-label={`Clear ${label.toLowerCase()}`}
        >
          <XIcon className="size-3.5 text-muted-foreground" />
        </Button>
      )}
    </div>
  )
}
