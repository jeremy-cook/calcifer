import { Link } from '@tanstack/react-router'
import { PlusIcon } from '@phosphor-icons/react'
import { Checkbox } from '~/components/ui/checkbox'
import { cn } from '~/lib/utils'
import { itemsOn, useCalendarIndex, type CalendarItem } from '~/model/calendar'
import { dailyNoteContentRef } from '~/model/daily-notes'
import {
  formatWeekRange,
  formatWeekday,
  shiftIso,
  useToday,
  weekNumber,
  weekStartIso,
  dateFromIso,
} from '~/model/dates'
import { richTextPlainText, useRichText } from '~/model/richtext'
import { useResolveDailyNote, type Entity } from '~/model/store'
import { structurePresentation } from '~/model/structures'
import { todoFields, useSetTodoStatus } from '~/model/todos'

interface WeekViewProps {
  iso: string
  controls: React.ReactNode
  onOpenDay: (iso: string) => void
}

// Monday to Sunday of `iso`'s week: each day's note, the ToDos due and the notes that
// mention it.
export function WeekView({ iso, controls, onOpenDay }: WeekViewProps) {
  const start = weekStartIso(iso)
  const days = Array.from({ length: 7 }, (_, i) => shiftIso(start, i))

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-3 border-b border-border px-6 py-3">
        <h1 className="font-heading text-2xl font-semibold text-heading">{formatWeekRange(start)}</h1>
        <span className="text-sm text-muted-foreground">Week {weekNumber(start)}</span>
        <div className="ml-auto">{controls}</div>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-7 overflow-y-auto">
        {days.map((day) => (
          <WeekDay key={day} iso={day} onOpenDay={onOpenDay} />
        ))}
      </div>
    </div>
  )
}

interface WeekDayProps {
  iso: string
  onOpenDay: (iso: string) => void
}

function WeekDay({ iso, onOpenDay }: WeekDayProps) {
  const today = useToday()
  const index = useCalendarIndex()
  const resolveDailyNote = useResolveDailyNote()

  const dailyNote = itemsOn(index, iso, 'dailyNote')[0]?.entity
  const due = itemsOn(index, iso, 'due')
  const references = itemsOn(index, iso, 'reference').filter((item) => item.entity.structureType !== 'DailyNote')
  const isToday = iso === today

  const handleNewNote = () => {
    void resolveDailyNote(iso)
    onOpenDay(iso)
  }

  const renderNewNote = () => (
    <button
      type="button"
      onClick={handleNewNote}
      className="flex items-center gap-1.5 rounded-md border border-dashed border-border px-2 py-1 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
    >
      <PlusIcon className="size-3.5" />
      Daily note
    </button>
  )

  return (
    <div
      className={cn(
        'flex min-w-0 flex-col gap-2 border-r border-border p-2.5 last:border-r-0',
        isToday && 'bg-primary/5',
      )}
    >
      <button
        type="button"
        onClick={() => onOpenDay(iso)}
        className="flex items-center gap-2 border-b border-border px-1 pb-2 text-left"
        aria-label={`Open ${formatWeekday(iso)}`}
      >
        <span
          className={cn('text-xs font-bold tracking-wider text-muted-foreground uppercase', isToday && 'text-primary')}
        >
          {formatWeekday(iso).slice(0, 3)}
        </span>
        <span
          className={cn(
            'ml-auto flex size-9 items-center justify-center rounded-full font-heading text-xl font-semibold text-secondary-foreground',
            isToday && 'bg-primary text-primary-foreground',
          )}
        >
          {dateFromIso(iso).getDate()}
        </span>
      </button>
      {dailyNote ? <DailyNotePreview dailyNote={dailyNote} onOpen={() => onOpenDay(iso)} /> : renderNewNote()}
      <WeekDayList label="Due" items={due} today={today} />
      <WeekDayList label="Mentioned" items={references} today={today} />
    </div>
  )
}

interface DailyNotePreviewProps {
  dailyNote: Entity
  onOpen: () => void
}

// The note's first lines; the full note is a click away in the Day view.
function DailyNotePreview({ dailyNote, onOpen }: DailyNotePreviewProps) {
  const ref = dailyNoteContentRef(dailyNote)
  const { data } = useRichText(ref!)
  const { icon: Icon, color } = structurePresentation('DailyNote')
  const text = richTextPlainText(data?.doc)

  return (
    <button
      type="button"
      onClick={onOpen}
      className="flex flex-col gap-0.5 rounded-md border-l-[3px] bg-card px-2 py-1.5 text-left hover:bg-accent"
      style={{ borderColor: color }}
    >
      <span className="flex items-center gap-1 text-[0.7rem] font-bold tracking-wider uppercase" style={{ color }}>
        <Icon weight="duotone" className="size-3.5" aria-hidden />
        Note
      </span>
      <span className={cn('line-clamp-4 text-[0.8125rem] leading-snug', !text && 'text-muted-foreground')}>
        {text || 'Empty'}
      </span>
    </button>
  )
}

interface WeekDayListProps {
  label: string
  items: readonly CalendarItem[]
  today: string
}

function WeekDayList({ label, items, today }: WeekDayListProps) {
  if (items.length === 0) return null

  return (
    <div className="flex flex-col">
      <span className="px-1.5 pb-0.5 text-[0.7rem] font-bold tracking-wider text-muted-foreground uppercase">
        {label}
      </span>
      {items.map((item) =>
        item.kind === 'due' ? (
          <WeekTodo key={item.entity.id} entity={item.entity} iso={item.iso} today={today} />
        ) : (
          <WeekEntity key={item.entity.id} entity={item.entity} />
        ),
      )}
    </div>
  )
}

interface WeekTodoProps {
  entity: Entity
  iso: string
  today: string
}

function WeekTodo({ entity, iso, today }: WeekTodoProps) {
  const setTodoStatus = useSetTodoStatus()
  const done = todoFields(entity).status === 'done'
  const overdue = !done && iso < today

  return (
    <div className="flex min-w-0 items-center gap-1.5 rounded px-1.5 py-1 text-[0.8125rem] hover:bg-accent">
      <Checkbox
        checked={done}
        onCheckedChange={(checked) => setTodoStatus(entity, checked === true ? 'done' : 'open')}
        aria-label={`Mark "${entity.name}" done`}
        className={cn('size-3.5', overdue && 'border-destructive')}
      />
      <Link
        to="/e/$id"
        params={{ id: entity.id }}
        className={cn('min-w-0 truncate', done && 'text-muted-foreground line-through', overdue && 'text-destructive')}
      >
        {entity.name}
      </Link>
    </div>
  )
}

interface WeekEntityProps {
  entity: Entity
}

function WeekEntity({ entity }: WeekEntityProps) {
  const { icon: Icon, color } = structurePresentation(entity.structureType)

  return (
    <Link
      to="/e/$id"
      params={{ id: entity.id }}
      className="flex min-w-0 items-center gap-1.5 rounded px-1.5 py-1 text-[0.8125rem] hover:bg-accent"
    >
      <Icon weight="duotone" className="size-3.5 shrink-0" style={{ color }} aria-hidden />
      <span className="truncate">{entity.name}</span>
    </Link>
  )
}
