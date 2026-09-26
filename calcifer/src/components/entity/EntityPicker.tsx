import { useState } from 'react'
import { PlusIcon } from '@phosphor-icons/react'
import { Command } from 'cmdk'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { useAllEntities, getOrCreateEntityForMention, type RelationTarget } from '~/model/store'
import { hasUniqueNames } from '~/model/structures'

export interface EntityPickerProps {
  // Empty: any structure (and no create option).
  targetStructure: string
  excludeIds: readonly string[]
  onPick: (target: RelationTarget) => void
  // The popover trigger; must accept a ref (e.g. a Button).
  children: React.ReactNode
}

// A searchable popover of entities to pick one relation target from, with a
// "Create" option for target structures whose names are unique.
export function EntityPicker({ targetStructure, excludeIds, onPick, children }: EntityPickerProps) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const entities = useAllEntities()

  const candidates = entities.filter(
    (e) => (!targetStructure || e.structureType === targetStructure) && !excludeIds.includes(e.id),
  )

  const pick = (target: RelationTarget) => {
    onPick(target)
    setQuery('')
    setOpen(false)
  }

  const canCreate = hasUniqueNames(targetStructure) && query.trim().length > 0

  const handleCreate = async () => {
    const name = query.trim()
    if (!name) return
    const entity = await getOrCreateEntityForMention(targetStructure, name)
    pick({ id: entity.id, structureType: entity.structureType })
  }

  const itemClassName =
    'flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground'

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-0">
        <Command shouldFilter={false} className="flex flex-col">
          <Command.Input
            autoFocus
            value={query}
            onValueChange={setQuery}
            placeholder="Search…"
            className="border-b border-border bg-transparent px-2.5 py-2 text-sm outline-none placeholder:text-muted-foreground"
          />
          <Command.List className="max-h-56 overflow-y-auto p-1">
            {candidates
              .filter((e) => e.name.toLowerCase().includes(query.trim().toLowerCase()))
              .map((e) => (
                <Command.Item
                  key={e.id}
                  value={e.id}
                  onSelect={() => pick({ id: e.id, structureType: e.structureType })}
                  className={itemClassName}
                >
                  {e.name}
                </Command.Item>
              ))}
            {canCreate && (
              <Command.Item value={`__create__${query}`} onSelect={() => void handleCreate()} className={itemClassName}>
                <PlusIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
                Create &quot;{query.trim()}&quot;
              </Command.Item>
            )}
            {candidates.length === 0 && !canCreate && (
              <Command.Empty className="px-2 py-1.5 text-sm text-muted-foreground">No results</Command.Empty>
            )}
          </Command.List>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
