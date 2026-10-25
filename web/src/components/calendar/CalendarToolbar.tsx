import { CalendarBlankIcon, CaretLeftIcon, CaretRightIcon } from '@phosphor-icons/react'
import { ToggleGroup, ToggleGroupItem } from '~/components/ui/toggle-group'
import { Button } from '~/components/ui/button'
import { shiftIso, todayIso } from '~/model/dates'

interface CalendarToolbarProps {
  iso: string
  onChangeIso: (iso: string) => void
}

export function CalendarToolbar({ iso, onChangeIso }: CalendarToolbarProps) {
  const viewModes = [
    { value: 'month', label: 'Month' },
    { value: 'week', label: 'Week' },
    { value: 'day', label: 'Day' },
  ] as const

  return (
    <div className="flex items-center gap-2 border-b border-border px-6 py-3">
      <div className="flex flex-1 justify-center">
        <ToggleGroup type="single" value="day">
          {viewModes.map((m) => (
            <ToggleGroupItem
              key={m.value}
              value={m.value}
              disabled={m.value !== 'day'}
              title={m.value !== 'day' ? 'Coming soon' : undefined}
              className="px-4"
            >
              {m.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      <div className="flex items-center gap-1">
        <Button variant="ghost" size="icon" aria-label="Previous day" onClick={() => onChangeIso(shiftIso(iso, -1))}>
          <CaretLeftIcon />
        </Button>
        <Button variant="ghost" size="sm" onClick={() => onChangeIso(todayIso())}>
          Today
        </Button>
        <Button variant="ghost" size="icon" aria-label="Next day" onClick={() => onChangeIso(shiftIso(iso, 1))}>
          <CaretRightIcon />
        </Button>
        <Button variant="outline" size="icon" aria-label="Open calendar" className="ml-1">
          <CalendarBlankIcon />
        </Button>
      </div>
    </div>
  )
}
