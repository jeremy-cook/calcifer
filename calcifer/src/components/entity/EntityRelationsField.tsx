import { useState } from 'react'
import { create as createMessage } from '@bufbuild/protobuf'
import { PlusIcon, XIcon } from '@phosphor-icons/react'
import { Command } from 'cmdk'
import { EntityRefSchema, type EntityRef } from '@calcifer/proto/calcifer/v1/entities_pb'
import { Badge } from '~/components/ui/badge'
import { Button } from '~/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { useAllEntities, getOrCreateEntityForMention, type CreatableStructureType } from '~/model/store'
import { hasUniqueNames } from '~/model/structures'

export interface EntityRelationsFieldProps {
  label: string
  targetStructure: string
  refs: readonly EntityRef[]
  onChange: (refs: EntityRef[]) => void
}

export function EntityRelationsField({ label, targetStructure, refs, onChange }: EntityRelationsFieldProps) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const entities = useAllEntities()

  const resolvedRefs = refs
    .map((ref) => ({ ref, entity: entities.find((e) => e.id === ref.id) }))
    .filter((r): r is { ref: EntityRef; entity: NonNullable<typeof r.entity> } => r.entity !== undefined)

  const candidates = entities.filter(
    (e) => e.structureType === targetStructure && !refs.some((r) => r.id === e.id),
  )

  const removeRef = (id: string) => {
    onChange(refs.filter((r) => r.id !== id))
  }

  const addRef = (id: string, structureType: string) => {
    onChange([...refs, createMessage(EntityRefSchema, { id, structureType })])
    setQuery('')
    setOpen(false)
  }

  const canCreate = hasUniqueNames(targetStructure) && query.trim().length > 0

  const handleCreate = async () => {
    const name = query.trim()
    if (!name) return
    const entity = await getOrCreateEntityForMention(targetStructure as CreatableStructureType, name)
    addRef(entity.id, entity.structureType)
  }

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
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <Button variant="ghost" size="sm" className="h-6 gap-1 px-1.5 text-xs font-normal text-muted-foreground">
              <PlusIcon className="size-3" />
              Add
            </Button>
          </PopoverTrigger>
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
                      onSelect={() => addRef(e.id, e.structureType)}
                      className="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground"
                    >
                      {e.name}
                    </Command.Item>
                  ))}
                {canCreate && (
                  <Command.Item
                    value={`__create__${query}`}
                    onSelect={() => void handleCreate()}
                    className="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground"
                  >
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
      </div>
    </div>
  )
}
