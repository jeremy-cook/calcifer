import { useNavigate } from '@tanstack/react-router'
import { shiftIso, todayIso } from '~/model/dates'
import { CalendarControls } from './CalendarControls'
import { DayView } from './DayView'
import { MiniCalendar } from './MiniCalendar'
import { WeekView } from './WeekView'

export type CalendarViewMode = 'day' | 'week'

interface CalendarPageProps {
  iso: string
  view: CalendarViewMode
}

export function CalendarPage({ iso, view }: CalendarPageProps) {
  const navigate = useNavigate()
  // Day is the default, so it stays out of the URL.
  const goTo = (date: string, nextView: CalendarViewMode = view) =>
    void navigate({
      to: '/calendar',
      search: { date, view: nextView === 'week' ? 'week' : undefined },
      replace: true,
    })

  const step = view === 'week' ? 7 : 1
  const controls = (
    <CalendarControls
      view={view}
      onView={(next) => goTo(iso, next)}
      onPrevious={() => goTo(shiftIso(iso, -step))}
      onToday={() => goTo(todayIso())}
      onNext={() => goTo(shiftIso(iso, step))}
    />
  )

  if (view === 'week') return <WeekView iso={iso} controls={controls} onOpenDay={(day) => goTo(day, 'day')} />

  return (
    // Without room for the month beside the day, the arrows and Today do its job.
    <div className="grid h-full min-h-0 grid-cols-1 @4xl:grid-cols-[minmax(0,1fr)_320px]">
      <DayView iso={iso} controls={controls} />
      <div className="hidden min-h-0 @4xl:block">
        <MiniCalendar iso={iso} onSelect={goTo} />
      </div>
    </div>
  )
}
