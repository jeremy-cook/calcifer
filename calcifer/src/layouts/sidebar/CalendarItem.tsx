import { CalendarBlankIcon } from '@phosphor-icons/react'
import { Link } from '@tanstack/react-router'

const ROW_CLASS =
  'flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground hover:bg-muted [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground'

export function CalendarItem() {
  return (
    <Link to="/calendar" className={ROW_CLASS} activeProps={{ className: 'bg-muted font-medium' }}>
      <CalendarBlankIcon />
      <span className="truncate">Calendar</span>
    </Link>
  )
}
