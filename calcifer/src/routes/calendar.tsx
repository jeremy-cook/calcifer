import { createFileRoute } from '@tanstack/react-router'
import { CalendarPage } from '~/components/calendar/CalendarPage'
import { todayIso } from '~/model/dates'

interface CalendarSearch {
  date?: string
}

export const Route = createFileRoute('/calendar')({
  validateSearch: (search: Record<string, unknown>): CalendarSearch => ({
    date: typeof search.date === 'string' ? search.date : undefined,
  }),
  component: function CalendarRoute() {
    const { date } = Route.useSearch()
    return <CalendarPage iso={date ?? todayIso()} />
  },
})
