import { createFileRoute } from '@tanstack/react-router'
import { CalendarPage, type CalendarViewMode } from '~/components/calendar/CalendarPage'
import { useToday } from '~/model/dates'

interface CalendarSearch {
  date?: string
  view?: CalendarViewMode
}

export const Route = createFileRoute('/calendar')({
  validateSearch: (search: Record<string, unknown>): CalendarSearch => ({
    date: typeof search.date === 'string' ? search.date : undefined,
    view: search.view === 'week' ? 'week' : undefined,
  }),
  component: function CalendarRoute() {
    const { date, view } = Route.useSearch()
    const today = useToday()
    return <CalendarPage iso={date ?? today} view={view ?? 'day'} />
  },
})
