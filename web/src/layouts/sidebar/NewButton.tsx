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
import { facetPresentation, structurePresentation, useStructures } from '~/model/structures'
import { TODO_FACET } from '~/model/todos'

const classnamesDropdownMenuTrigger = [
  'flex w-full items-center',
  'mb-2 gap-2.5 rounded-md px-2.5 py-1.5',
  'bg-muted text-left text-sm font-bold text-primary',
  'hover:bg-sidebar-accent aria-expanded:bg-sidebar-accent',
  '[&_svg]:size-[18px] [&_svg]:shrink-0',
].join(' ')

export function NewButton() {
  const navigate = useNavigate()
  const createEntity = useCreateEntity()
  const creatable = useStructures().filter((s) => s.creatable)

  const { icon: TodoIcon, color: todoColor } = facetPresentation(TODO_FACET)

  const handleCreate = async (structureType: string, facets?: string[]) => {
    const entity = await createEntity(structureType, { facets })
    void navigate({ to: '/e/$id', params: { id: entity.id }, state: { justCreated: true } })
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger className={classnamesDropdownMenuTrigger}>
        <PlusIcon weight="bold" />
        New
      </DropdownMenuTrigger>
      <DropdownMenuContent side="right" align="start" sideOffset={8}>
        <DropdownMenuGroup>
          {creatable.map((s) => {
            const { icon: IconComponent, color } = structurePresentation(s.type)
            return (
              <DropdownMenuItem key={s.type} onSelect={() => void handleCreate(s.type)}>
                <IconComponent weight="duotone" style={{ color }} />
                {s.name}
              </DropdownMenuItem>
            )
          })}
          <DropdownMenuItem onSelect={() => void handleCreate('Note', [TODO_FACET])}>
            <TodoIcon weight="duotone" style={{ color: todoColor }} />
            To-do
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
