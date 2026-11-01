import { useState } from 'react'
import { CaretDownIcon, CaretRightIcon } from '@phosphor-icons/react'
import { Checkbox } from '~/components/ui/checkbox'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { cn } from '~/lib/utils'
import { formatMonth, useToday } from '~/model/dates'
import type { Entity } from '~/model/store'
import {
  TODO_GROUPS,
  todoFields,
  todoGroup,
  todoMonth,
  type TodoDueKey,
  type TodoGroupKey,
  type TodoMonthKey,
  type TodoSearch,
} from '~/model/todos'

export interface TodoDueFilterProps {
  due: TodoDueKey[]
  // The ToDos the other filters keep, for each group's and month's count.
  anyDue: Entity[]
  onChange: (next: Partial<TodoSearch>) => void
}

const isMonth = (key: TodoDueKey): key is TodoMonthKey => !TODO_GROUPS.some((g) => g.key === key)

// Tick due groups to show only those; nothing ticked is any due date. Later opens
// into the months that have ToDos due, to tick one on its own. Done is left to the
// Open / Done / All switch.
export function TodoDueFilter({ due, anyDue, onChange }: TodoDueFilterProps) {
  const today = useToday()
  const counts = new Map<TodoDueKey, number>()
  for (const entity of anyDue) {
    const fields = todoFields(entity)
    const group = todoGroup(fields, today)
    const month = todoMonth(fields, today)
    counts.set(group, (counts.get(group) ?? 0) + 1)
    if (month) counts.set(month, (counts.get(month) ?? 0) + 1)
  }

  // A ticked month stays listed after its last ToDo leaves, so it can be unticked.
  const tickedMonths = due.filter(isMonth)
  const months = [...new Set([...counts.keys()].filter(isMonth).concat(tickedMonths))].sort()
  const [open, setOpen] = useState(tickedMonths.length > 0)

  const laterOn = due.includes('later')
  const allMonths = months.length > 0 && months.every((m) => tickedMonths.includes(m))
  const laterState = laterOn || allMonths ? true : tickedMonths.length > 0 ? 'indeterminate' : false
  const monthOn = (month: TodoMonthKey) => laterOn || tickedMonths.includes(month)

  const groups = TODO_GROUPS.filter((g) => g.key !== 'done')

  // Page order, with ticked months where Later sits, so the label reads the same
  // however it was ticked.
  const inPageOrder = (keys: TodoDueKey[]): TodoDueKey[] =>
    TODO_GROUPS.flatMap((g) => [
      ...(keys.includes(g.key) ? [g.key] : []),
      ...(g.key === 'later' ? keys.filter(isMonth).sort() : []),
    ])

  const set = (keys: TodoDueKey[]) => {
    const next = inPageOrder(keys)
    onChange({ due: next.length > 0 ? next : undefined })
  }
  const others = due.filter((k) => k !== 'later' && !isMonth(k))

  const toggleGroup = (key: TodoGroupKey, on: boolean) => {
    if (key === 'later') set(on ? [...others, 'later'] : others)
    else set(on ? [...due, key] : due.filter((k) => k !== key))
  }

  // Unticking one month while all of Later is ticked keeps the rest of its months.
  const toggleMonth = (month: TodoMonthKey, on: boolean) => {
    const ticked = laterOn ? months : tickedMonths
    set([...others, ...(on ? [...ticked, month] : ticked.filter((m) => m !== month))])
  }

  const labelOf = (key: TodoDueKey) => (isMonth(key) ? formatMonth(key) : groups.find((g) => g.key === key)?.label)
  const label = due.length === 0 ? 'Any due date' : inPageOrder(due).map(labelOf).join(', ')
  const Twist = open ? CaretDownIcon : CaretRightIcon

  const renderLaterRow = () => (
    <>
      <div className="flex h-8 items-center rounded-[9px] hover:bg-primary/15">
        <button
          type="button"
          onClick={() => setOpen(!open)}
          disabled={months.length === 0}
          aria-label={open ? 'Hide months' : 'Show months'}
          aria-expanded={open}
          className="flex h-full w-6 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground disabled:invisible"
        >
          <Twist weight="bold" className="size-3" />
        </button>
        <DueOption
          label="Later"
          count={counts.get('later') ?? 0}
          checked={laterState}
          onCheckedChange={(on) => toggleGroup('later', on)}
          className="pl-0"
        />
      </div>
      {open &&
        months.map((month) => (
          <DueOption
            key={month}
            label={formatMonth(month)}
            count={counts.get(month) ?? 0}
            checked={monthOn(month)}
            onCheckedChange={(on) => toggleMonth(month, on)}
            className="h-7 pl-12 hover:bg-primary/15"
          />
        ))}
    </>
  )

  return (
    <Popover>
      <PopoverTrigger
        className={cn(
          'flex h-8 max-w-72 items-center gap-1.5 rounded-md border border-input bg-input/30 py-2 pr-2 pl-3 text-sm whitespace-nowrap hover:bg-input/50',
          due.length > 0 && 'border-primary/60',
        )}
      >
        <span className="truncate">{label}</span>
        <CaretDownIcon className="size-4 shrink-0 text-muted-foreground" />
      </PopoverTrigger>
      <PopoverContent align="start" className="w-auto min-w-60 p-1.5">
        {groups.map(({ key, label }) =>
          key === 'later' ? (
            <div key={key}>{renderLaterRow()}</div>
          ) : (
            <DueOption
              key={key}
              label={label}
              count={counts.get(key) ?? 0}
              checked={due.includes(key)}
              onCheckedChange={(on) => toggleGroup(key, on)}
              className="pl-6 hover:bg-primary/15"
            />
          ),
        )}
      </PopoverContent>
    </Popover>
  )
}

interface DueOptionProps {
  label: string
  count: number
  checked: boolean | 'indeterminate'
  onCheckedChange: (on: boolean) => void
  className?: string
}

function DueOption({ label, count, checked, onCheckedChange, className }: DueOptionProps) {
  return (
    <label className={cn('flex h-8 flex-1 cursor-default items-center gap-2.5 rounded-[9px] pr-2 text-sm', className)}>
      <Checkbox checked={checked} onCheckedChange={(next) => onCheckedChange(next === true)} />
      {label}
      <span className="ml-auto pl-4 text-xs text-muted-foreground">{count}</span>
    </label>
  )
}
