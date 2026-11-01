import { CalendarCheckIcon } from '@phosphor-icons/react'
import { itemsOn, useCalendarIndex } from '~/model/calendar'
import { facetPresentation } from '~/model/structures'
import { TODO_FACET } from '~/model/todos'
import { DayHeading } from './DayHeading'
import { DailyNoteSection } from './DailyNoteSection'
import { CalendarItemsSection } from './CalendarItemsSection'
import { CreatedAt } from './CreatedAt'

interface DayViewProps {
  iso: string
}

export function DayView({ iso }: DayViewProps) {
  const index = useCalendarIndex()
  const due = itemsOn(index, iso, 'due')
  // Daily notes have their own section above, so they're left out of references.
  const todo = facetPresentation(TODO_FACET)
  const references = itemsOn(index, iso, 'reference').filter((item) => item.entity.structureType !== 'DailyNote')

  return (
    <div className="flex flex-col gap-6 overflow-y-auto px-12 py-8">
      <DayHeading iso={iso} />
      <Divider />
      <DailyNoteSection iso={iso} />
      <Divider />
      <CalendarItemsSection title="Due" icon={todo.icon} color={todo.color} emptyText="No ToDos due." items={due} />
      <Divider />
      <CalendarItemsSection
        title="Date references"
        icon={CalendarCheckIcon}
        color="var(--chart-1)"
        emptyText="No date references."
        items={references}
      />
      <Divider />
      <CreatedAt />
    </div>
  )
}

function Divider() {
  return <hr className="border-border" />
}
