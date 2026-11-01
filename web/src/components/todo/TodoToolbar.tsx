import { ToggleGroup, ToggleGroupItem } from '~/components/ui/toggle-group'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '~/components/ui/select'
import { useEntitiesOfType, type Entity } from '~/model/store'
import { useFacet } from '~/model/structures'
import {
  TODO_FACET,
  todoPriorityOptions,
  todoStatusFilterOptions,
  type TodoFilter,
  type TodoOrder,
  type TodoSearch,
} from '~/model/todos'
import { TodoDueFilter } from './TodoDueFilter'
import { TodoOrderChips } from './TodoOrderChips'

export interface TodoToolbarProps {
  filter: TodoFilter
  order: TodoOrder
  // The ToDos every filter but the due one keeps.
  anyDue: Entity[]
  onChange: (next: Partial<TodoSearch>) => void
}

const NONE = '__none__'

export function TodoToolbar({ filter, order, anyDue, onChange }: TodoToolbarProps) {
  const todo = useFacet(TODO_FACET)
  const tags = useEntitiesOfType('Tag')
  const statusFilterOptions = todoStatusFilterOptions(todo)
  const priorityOptions = todoPriorityOptions(todo)

  return (
    <div className="flex flex-wrap items-center gap-2">
      <ToggleGroup
        type="single"
        variant="segmented"
        spacing={0.5}
        value={filter.status}
        onValueChange={(v) => {
          // Radix emits '' when the pressed item is clicked again — ignore so the filter can't be cleared.
          if (v) onChange({ status: v })
        }}
      >
        {statusFilterOptions.map((item) => (
          <ToggleGroupItem key={item.key} value={item.key}>
            {item.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>

      <Select value={filter.priority ?? NONE} onValueChange={(v) => onChange({ priority: v === NONE ? undefined : v })}>
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

      <Select value={filter.tag ?? NONE} onValueChange={(v) => onChange({ tag: v === NONE ? undefined : v })}>
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

      <TodoDueFilter due={filter.due ?? []} anyDue={anyDue} onChange={onChange} />

      <div className="ml-auto">
        <TodoOrderChips order={order} onChange={onChange} />
      </div>
    </div>
  )
}
