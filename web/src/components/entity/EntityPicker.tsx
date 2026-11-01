import { useState } from 'react'
import { PlusIcon } from '@phosphor-icons/react'
import { Command } from 'cmdk'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { useAllEntities, getOrCreateEntityForMention, type RelationTarget } from '~/model/store'
import { hasUniqueNames } from '~/model/structures'

export interface EntityPickerProps {
  // One structure to pick from; empty or omitted: any (and no create option). Ignored
  // while `targetStructures` is non-empty.
  targetStructure?: string
  // Several structures to pick from; empty or omitted: `targetStructure` decides.
  targetStructures?: readonly string[]
  excludeIds: readonly string[]
  onPick: (target: RelationTarget) => void
  // The popover trigger; must accept a ref (e.g. a Button).
  children: React.ReactNode
}

const ANY_STRUCTURE: readonly string[] = []

// A searchable popover of entities to pick one relation target from, with a
// "Create" option when the only structure on offer has unique names.
export function EntityPicker({
  targetStructure = '',
  targetStructures = ANY_STRUCTURE,
  excludeIds,
  onPick,
  children,
}: EntityPickerProps) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')

  // Empty: any structure.
  const structures =
    targetStructures.length > 0 ? targetStructures : targetStructure ? [targetStructure] : ANY_STRUCTURE
  // The structure "Create" makes an entity of; empty: none (any, or more than one).
  const createStructure = structures.length === 1 ? structures[0] : ''

  const pick = (target: RelationTarget) => {
    onPick(target)
    setQuery('')
    setOpen(false)
  }

  const handleCreate = async () => {
    const name = query.trim()
    if (!name) return
    const entity = await getOrCreateEntityForMention(createStructure, name)
    pick({ id: entity.id, structureType: entity.structureType })
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-1.5">
        <Command shouldFilter={false} className="flex flex-col">
          <Command.Input
            autoFocus
            value={query}
            onValueChange={setQuery}
            placeholder="Search…"
            className="mb-1 rounded-[9px] bg-background px-2.5 py-2 text-sm outline-none placeholder:text-muted-foreground"
          />
          <EntityPickerList
            structures={structures}
            createStructure={createStructure}
            excludeIds={excludeIds}
            query={query}
            onPick={pick}
            onCreate={() => void handleCreate()}
          />
        </Command>
      </PopoverContent>
    </Popover>
  )
}

interface EntityPickerListProps {
  structures: readonly string[]
  createStructure: string
  excludeIds: readonly string[]
  query: string
  onPick: (target: RelationTarget) => void
  onCreate: () => void
}

// The picker's results. Mounted only while the popover is open, so a closed
// picker doesn't re-render on every write to the replica.
function EntityPickerList({ structures, createStructure, excludeIds, query, onPick, onCreate }: EntityPickerListProps) {
  const entities = useAllEntities()

  const candidates = entities.filter(
    (e) => (structures.length === 0 || structures.includes(e.structureType)) && !excludeIds.includes(e.id),
  )

  const canCreate = hasUniqueNames(createStructure) && query.trim().length > 0

  const itemClassName =
    'flex cursor-default items-center gap-2 rounded-[9px] px-2 py-1.5 text-sm outline-none data-[selected=true]:bg-primary/15 data-[selected=true]:text-accent-foreground'

  return (
    <Command.List className="max-h-56 overflow-y-auto">
      {candidates
        .filter((e) => e.name.toLowerCase().includes(query.trim().toLowerCase()))
        .map((e) => (
          <Command.Item
            key={e.id}
            value={e.id}
            onSelect={() => onPick({ id: e.id, structureType: e.structureType })}
            className={itemClassName}
          >
            {e.name}
          </Command.Item>
        ))}
      {canCreate && (
        <Command.Item value={`__create__${query}`} onSelect={onCreate} className={itemClassName}>
          <PlusIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
          Create &quot;{query.trim()}&quot;
        </Command.Item>
      )}
      {candidates.length === 0 && !canCreate && (
        <Command.Empty className="px-2 py-1.5 text-sm text-muted-foreground">No results</Command.Empty>
      )}
    </Command.List>
  )
}
