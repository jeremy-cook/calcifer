import { SortAscendingIcon, SortDescendingIcon } from '@phosphor-icons/react'
import { ToggleGroup, ToggleGroupItem } from '~/components/ui/toggle-group'
import { Toggle } from '~/components/ui/toggle'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '~/components/ui/select'
import { useAllEntities } from '~/model/store'
import { useStructure } from '~/model/structures'
import {
  TODO_DUE_FILTER_OPTIONS,
  TODO_SORT_OPTIONS,
  todoPriorityOptions,
  todoStatusFilterOptions,
  type TodoFilter,
  type TodoSearch,
  type TodoSort,
} from '~/model/todos'

export interface TodoToolbarProps {
  filter: TodoFilter
  sort: TodoSort
  onChange: (next: Partial<TodoSearch>) => void
}

const NONE = '__none__'

export function TodoToolbar({ filter, sort, onChange }: TodoToolbarProps) {
  const entities = useAllEntities()
  const todo = useStructure('Todo')
  const tags = entities.filter((e) => e.structureType === 'Tag')
  const statusFilterOptions = todoStatusFilterOptions(todo)
  const priorityOptions = todoPriorityOptions(todo)

  return (
    <div className="flex flex-wrap items-center gap-2">
      <ToggleGroup
        type="single"
        variant="outline"
        value={filter.status}
        onValueChange={(v) => {
          // Radix emits '' when the pressed item is clicked again — ignore so the filter can't be cleared.
          if (v) onChange({ status: v })
        }}
      >
        {statusFilterOptions.map((item) => (
          <ToggleGroupItem key={item.key} value={item.key} className="px-3">
            {item.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>

      <Select
        value={filter.priority ?? NONE}
        onValueChange={(v) => onChange({ priority: v === NONE ? undefined : v })}
      >
        <SelectTrigger size="sm" className="h-8">
          <SelectValue placeholder="Priority" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE}>Any priority</SelectItem>
          {priorityOptions.map((option) => (
            <SelectItem key={option.key} value={option.key}>
              {option.key === 'none' ? 'No priority' : option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Select
        value={filter.tag ?? NONE}
        onValueChange={(v) => onChange({ tag: v === NONE ? undefined : v })}
      >
        <SelectTrigger size="sm" className="h-8">
          <SelectValue placeholder="Tag" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE}>Any tag</SelectItem>
          {tags.map((tag) => (
            <SelectItem key={tag.id} value={tag.id}>
              {tag.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Select
        value={filter.due ?? NONE}
        onValueChange={(v) => onChange({ due: v === NONE ? undefined : (v as TodoFilter['due']) })}
      >
        <SelectTrigger size="sm" className="h-8">
          <SelectValue placeholder="Due" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE}>Any due date</SelectItem>
          {TODO_DUE_FILTER_OPTIONS.map((item) => (
            <SelectItem key={item.key} value={item.key}>
              {item.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <div className="ml-auto flex items-center gap-1">
        <Select value={sort.by} onValueChange={(v) => onChange({ sort: v as TodoSort['by'] })}>
          <SelectTrigger size="sm" className="h-8">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {TODO_SORT_OPTIONS.map((item) => (
              <SelectItem key={item.key} value={item.key}>
                {item.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Toggle
          size="sm"
          pressed={sort.dir === 'desc'}
          onPressedChange={(pressed) => onChange({ dir: pressed ? 'desc' : 'asc' })}
          aria-label="Toggle sort direction"
        >
          {sort.dir === 'desc' ? <SortDescendingIcon /> : <SortAscendingIcon />}
        </Toggle>
      </div>
    </div>
  )
}
