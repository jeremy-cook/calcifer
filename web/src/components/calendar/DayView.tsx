import { CalendarCheckIcon, SparkleIcon } from '@phosphor-icons/react'
import { itemsOn, useCalendarIndex } from '~/model/calendar'
import { facetPresentation } from '~/model/structures'
import { TODO_FACET } from '~/model/todos'
import { DayHeading } from './DayHeading'
import { DailyNoteSection } from './DailyNoteSection'
import { CalendarItemsSection } from './CalendarItemsSection'

interface DayViewProps {
  iso: string
  // The view switch and day stepping, set beside the heading.
  controls: React.ReactNode
}

export function DayView({ iso, controls }: DayViewProps) {
  const index = useCalendarIndex()
  const todo = facetPresentation(TODO_FACET)
  const due = itemsOn(index, iso, 'due')
  // Daily notes have their own section above, so they're left out of references.
  const references = itemsOn(index, iso, 'reference').filter((item) => item.entity.structureType !== 'DailyNote')
  const created = itemsOn(index, iso, 'created')

  return (
    <div className="flex min-h-0 flex-col overflow-y-auto">
      <div className="flex items-end gap-4 px-7 @3xl:px-16 pt-7 pb-5">
        <DayHeading iso={iso} />
        <div className="ml-auto">{controls}</div>
      </div>
      <DailyNoteSection iso={iso} />
      <div className="grid grid-cols-1 gap-6 border-t border-border px-7 @3xl:px-16 py-5 @3xl:grid-cols-3 @3xl:gap-8">
        <CalendarItemsSection title="Due" icon={todo.icon} color={todo.color} items={due} />
        <CalendarItemsSection title="Mentioned" icon={CalendarCheckIcon} color="var(--chart-1)" items={references} />
        <CalendarItemsSection title="Created this day" icon={SparkleIcon} color="var(--chart-2)" items={created} />
      </div>
    </div>
  )
}
