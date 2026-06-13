import { useEffect, useMemo, useState } from 'react'
import { Calendar } from '~/components/ui/calendar'
import { dateFromIso, isoFromDate } from '~/model/dates'
import { daysWithContent, useAllEntities } from '~/model/store'

interface MiniCalendarProps {
  iso: string
  onSelect: (iso: string) => void
}

const HAS_CONTENT_CLASSES =
  'after:pointer-events-none after:absolute after:bottom-1 after:left-1/2 after:z-20 after:size-1 after:-translate-x-1/2 after:rounded-full after:bg-foreground after:opacity-60 after:content-[""]'

export function MiniCalendar({ iso, onSelect }: MiniCalendarProps) {
  const selected = dateFromIso(iso)
  const [month, setMonth] = useState(selected)
  const entities = useAllEntities()
  const marked = useMemo(() => daysWithContent(entities), [entities])

  useEffect(() => {
    setMonth(dateFromIso(iso))
  }, [iso])

  const handleSelect = (date: Date | undefined) => {
    if (date) onSelect(isoFromDate(date))
  }

  return (
    <div className="min-w-0 overflow-hidden border-l border-border p-4">
      <Calendar
        mode="single"
        selected={selected}
        onSelect={handleSelect}
        month={month}
        onMonthChange={setMonth}
        weekStartsOn={1}
        captionLayout="dropdown"
        showOutsideDays
        fixedWeeks
        modifiers={{ hasContent: (date) => marked.has(isoFromDate(date)) }}
        modifiersClassNames={{ hasContent: HAS_CONTENT_CLASSES }}
        className="w-full [--cell-size:--spacing(10)]"
        classNames={{
          dropdowns: 'flex items-center justify-center gap-4 text-sm font-medium',
          weekday: 'flex-1 rounded-(--cell-radius) text-[0.8rem] font-normal text-chart-1 select-none',
          week: 'mt-1 flex w-full',
          day: 'group/day relative h-full w-full rounded-(--cell-radius) p-0 text-center select-none',
          day_button: 'aspect-auto h-(--cell-size)',
        }}
      />
    </div>
  )
}
