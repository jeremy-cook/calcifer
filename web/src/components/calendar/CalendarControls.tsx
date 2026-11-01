import { CaretLeftIcon, CaretRightIcon } from '@phosphor-icons/react'
import { Button } from '~/components/ui/button'
import { ToggleGroup, ToggleGroupItem } from '~/components/ui/toggle-group'
import type { CalendarViewMode } from './CalendarPage'

interface CalendarControlsProps {
  view: CalendarViewMode
  onView: (view: CalendarViewMode) => void
  onPrevious: () => void
  onToday: () => void
  onNext: () => void
}

// The Week/Day switch and ‹ Today ›, which step a day or a week to match the view.
export function CalendarControls({ view, onView, onPrevious, onToday, onNext }: CalendarControlsProps) {
  const viewModes = [
    { value: 'week', label: 'Week' },
    { value: 'day', label: 'Day' },
  ] as const
  const unit = view === 'week' ? 'week' : 'day'

  return (
    <div className="flex shrink-0 items-center gap-3">
      <ToggleGroup
        type="single"
        variant="segmented"
        spacing={0.5}
        value={view}
        onValueChange={(next) => next && onView(next as CalendarViewMode)}
      >
        {viewModes.map((m) => (
          <ToggleGroupItem key={m.value} value={m.value}>
            {m.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      <div className="flex items-center gap-1">
        <Button variant="ghost" size="icon" aria-label={`Previous ${unit}`} onClick={onPrevious}>
          <CaretLeftIcon />
        </Button>
        <Button variant="outline" size="sm" onClick={onToday}>
          Today
        </Button>
        <Button variant="ghost" size="icon" aria-label={`Next ${unit}`} onClick={onNext}>
          <CaretRightIcon />
        </Button>
      </div>
    </div>
  )
}
