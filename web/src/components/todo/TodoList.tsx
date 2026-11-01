import { useMemo } from 'react'
import { CaretDownIcon, CheckCircleIcon, FunnelIcon } from '@phosphor-icons/react'
import { EmptyState } from '~/components/empty-state/EmptyState'
import { Button } from '~/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '~/components/ui/collapsible'
import { cn } from '~/lib/utils'
import { formatMonth, useToday } from '~/model/dates'
import { useEntitiesOfType, type Entity } from '~/model/store'
import { groupTodos, todoFields, todoMonth, type TodoGroup, type TodoMonthKey } from '~/model/todos'
import { TodoRow } from './TodoRow'

export interface TodoListProps {
  // In group order, as selectTodos returns them.
  entities: Entity[]
  // Whether the filters differ from the defaults, and how to reset them.
  filtered: boolean
  onClearFilters: () => void
}

export function TodoList({ entities, filtered, onClearFilters }: TodoListProps) {
  const today = useToday()
  const tags = useEntitiesOfType('Tag')
  const tagsById = useMemo(() => new Map(tags.map((tag) => [tag.id, tag])), [tags])

  if (entities.length === 0 && filtered) {
    return (
      <EmptyState
        icon={FunnelIcon}
        color="var(--chart-4)"
        title="Nothing matches"
        description="No ToDos match these filters."
        action={
          <Button variant="outline" onClick={onClearFilters}>
            Clear filters
          </Button>
        }
      />
    )
  }

  if (entities.length === 0) {
    return (
      <EmptyState
        icon={CheckCircleIcon}
        color="var(--chart-4)"
        title="All clear"
        description="No open ToDos. Add one above."
      />
    )
  }

  const resolveTags = (entity: Entity) =>
    todoFields(entity)
      .tagIds.map((id) => tagsById.get(id))
      .filter((e): e is Entity => e !== undefined)

  return (
    <div className="flex max-w-4xl flex-col gap-3">
      {groupTodos(entities, today).map((group) => (
        <TodoGroupSection key={group.key} group={group}>
          {/* Later is labelled by month, matching the months in the due filter. */}
          {(group.key === 'later' ? byMonth(group.todos, today) : [{ month: undefined, todos: group.todos }]).map(
            ({ month, todos }) => (
              <div key={month ?? group.key}>
                {month && (
                  <div className="flex h-6 items-end px-2.5 pl-8 text-xs font-semibold text-muted-foreground">
                    {formatMonth(month)}
                  </div>
                )}
                <ul className="flex flex-col">
                  {todos.map((entity) => (
                    <TodoRow key={entity.id} entity={entity} today={today} tags={resolveTags(entity)} />
                  ))}
                </ul>
              </div>
            ),
          )}
        </TodoGroupSection>
      ))}
    </div>
  )
}

interface MonthRun {
  month: TodoMonthKey | undefined
  todos: Entity[]
}

// Runs of ToDos by month; selectTodos already sorts Later by month.
function byMonth(todos: Entity[], today: string): MonthRun[] {
  const runs: MonthRun[] = []
  for (const entity of todos) {
    const month = todoMonth(todoFields(entity), today)
    const last = runs.at(-1)
    if (last && last.month === month) last.todos.push(entity)
    else runs.push({ month, todos: [entity] })
  }
  return runs
}

interface TodoGroupSectionProps {
  group: TodoGroup
  children: React.ReactNode
}

// A group's heading folds it away; a fold lasts until the page is left.
function TodoGroupSection({ group, children }: TodoGroupSectionProps) {
  return (
    <Collapsible defaultOpen asChild>
      <section>
        <CollapsibleTrigger
          className={cn(
            'group flex h-6 w-full items-center gap-2 px-2.5 text-xs font-bold tracking-wider text-muted-foreground uppercase after:ml-1 after:h-px after:flex-1 after:bg-border hover:text-foreground',
            group.key === 'overdue' && 'text-destructive hover:text-destructive',
          )}
        >
          <CaretDownIcon weight="bold" className="size-3 transition-transform group-data-[state=closed]:-rotate-90" />
          {group.label}
          <span className="font-medium tracking-normal normal-case">· {group.todos.length}</span>
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-0.5">{children}</CollapsibleContent>
      </section>
    </Collapsible>
  )
}
