import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/calendar')({
  component: CalendarPage,
})

function CalendarPage() {
  return (
    <div className="flex flex-col gap-2 px-16 py-10">
      <h1 className="text-2xl font-semibold">Calendar</h1>
      <p className="text-sm text-muted-foreground">Coming in Phase 6.</p>
    </div>
  )
}
