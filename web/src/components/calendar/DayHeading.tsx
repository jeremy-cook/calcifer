import { formatLongDate, formatWeekday, weekNumber } from '~/model/dates'

interface DayHeadingProps {
  iso: string
}

export function DayHeading({ iso }: DayHeadingProps) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-sm font-medium text-chart-1">{formatWeekday(iso)}</p>
      <div className="flex items-baseline gap-3">
        <h1 className="text-4xl font-bold tracking-tight">{formatLongDate(iso)}</h1>
        <span className="text-sm text text-chart-1">Week {weekNumber(iso)}</span>
      </div>
    </div>
  )
}
