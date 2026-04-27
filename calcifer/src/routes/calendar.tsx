import { createFileRoute } from '@tanstack/react-router'
import { CalendarPage } from '~/components/calendar/CalendarPage'

export const Route = createFileRoute('/calendar')({
  component: CalendarPage,
})
