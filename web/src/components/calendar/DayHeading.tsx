import { formatDayTitle, weekNumber } from '~/model/dates'

interface DayHeadingProps {
  iso: string
}

export function DayHeading({ iso }: DayHeadingProps) {
  return (
    <div className="flex flex-col gap-1">
      <h1 className="font-heading text-4xl font-semibold tracking-tight whitespace-nowrap text-heading">
        {formatDayTitle(iso)}
      </h1>
      <span className="text-xs font-semibold text-muted-foreground">Week {weekNumber(iso)}</span>
    </div>
  )
}
