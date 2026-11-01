import { formatLongDate, formatWeekday, weekNumber } from '~/model/dates'

interface DayHeadingProps {
  iso: string
}

export function DayHeading({ iso }: DayHeadingProps) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs font-bold tracking-widest text-primary uppercase">{formatWeekday(iso)}</p>
      <h1 className="font-heading text-4xl font-semibold tracking-tight text-heading">{formatLongDate(iso)}</h1>
      <span className="text-xs font-semibold text-muted-foreground">Week {weekNumber(iso)}</span>
    </div>
  )
}
