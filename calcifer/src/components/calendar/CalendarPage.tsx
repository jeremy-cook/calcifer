import { useNavigate } from '@tanstack/react-router'
import { CalendarToolbar } from './CalendarToolbar'
import { DayView } from './DayView'
import { MiniCalendar } from './MiniCalendar'

interface CalendarPageProps {
  iso: string
}

export function CalendarPage({ iso }: CalendarPageProps) {
  const navigate = useNavigate()
  const goTo = (next: string) =>
    void navigate({ to: '/calendar', search: { date: next }, replace: true })

  return (
    <div className="flex h-full flex-col">
      <CalendarToolbar iso={iso} onChangeIso={goTo} />
      <div className="grid min-h-0 flex-1 grid-cols-[1fr_320px]">
        <DayView iso={iso} />
        <MiniCalendar iso={iso} onSelect={goTo} />
      </div>
    </div>
  )
}
