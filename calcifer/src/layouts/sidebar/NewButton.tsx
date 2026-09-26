import { PlusIcon } from '@phosphor-icons/react'
import { useNavigate } from '@tanstack/react-router'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '~/components/ui/dropdown-menu'
import { useCreateEntity } from '~/model/store'
import { structurePresentation, useStructures } from '~/model/structures'

const classnamesDropdownMenuTrigger = [
  'flex w-full items-center gap-2',
  'rounded-md px-2 py-1.5',
  'text-left text-sm text-foreground',
  'hover:bg-muted aria-expanded:bg-muted',
  '[&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground',
].join(' ')

export function NewButton() {
  const navigate = useNavigate()
  const createEntity = useCreateEntity()
  const creatable = useStructures().filter((s) => s.creatable)

  const handleCreate = async (structureType: string) => {
    const entity = await createEntity(structureType)
    void navigate({ to: '/e/$id', params: { id: entity.id }, state: { justCreated: true } })
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger className={classnamesDropdownMenuTrigger}>
        <PlusIcon />
        New
      </DropdownMenuTrigger>
      <DropdownMenuContent side="right" align="start" sideOffset={8}>
        <DropdownMenuGroup>
          {creatable.map((s) => {
            const IconComponent = structurePresentation(s.type).icon
            return (
              <DropdownMenuItem key={s.type} onSelect={() => void handleCreate(s.type)}>
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
