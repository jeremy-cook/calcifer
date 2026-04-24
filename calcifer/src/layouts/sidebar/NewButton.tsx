import { PlusIcon } from '@phosphor-icons/react'
import { useNavigate } from '@tanstack/react-router'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '~/components/ui/dropdown-menu'
import { useEntityStore } from '~/model/store'
import type { StructureType } from '~/model/structures'
import { CREATABLE_STRUCTURES } from '~/model/structures'

const classnamesDropdownMenuTrigger = [
  'flex w-full items-center gap-2',
  'rounded-md px-2 py-1.5',
  'text-left text-sm text-foreground',
  'hover:bg-muted aria-expanded:bg-muted',
  '[&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground',
].join(' ')

export function NewButton() {
  const navigate = useNavigate()
  const createEntity = useEntityStore((s) => s.createEntity)

  const handleCreate = (structureType: StructureType) => {
    const entity = createEntity(structureType as Exclude<StructureType, 'DailyNote'>)
    void navigate({ to: '/e/$id', params: { id: entity.id } })
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger className={classnamesDropdownMenuTrigger}>
        <PlusIcon />
        New
      </DropdownMenuTrigger>
      <DropdownMenuContent side="right" align="start" sideOffset={8}>
        <DropdownMenuGroup>
          {CREATABLE_STRUCTURES.map((s) => {
            const IconComponent = s.icon
            return (
              <DropdownMenuItem key={s.type} onSelect={() => handleCreate(s.type as StructureType)}>
                <IconComponent />
                {s.name}
              </DropdownMenuItem>
            )
          })}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
