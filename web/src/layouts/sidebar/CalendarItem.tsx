import { CalendarBlankIcon } from '@phosphor-icons/react'
import { Link } from '@tanstack/react-router'
import { cn } from '~/lib/utils'
import { NAV_ROW_ACTIVE_CLASS, NAV_ROW_CLASS, NAV_ROW_RAIL_CLASS } from './nav-row'

export interface CalendarItemProps {
  rail: boolean
}

export function CalendarItem({ rail }: CalendarItemProps) {
  return (
    <Link
      to="/calendar"
      title={rail ? 'Calendar' : undefined}
      activeProps={{ className: NAV_ROW_ACTIVE_CLASS }}
      className={cn(NAV_ROW_CLASS, rail && NAV_ROW_RAIL_CLASS)}
    >
      <CalendarBlankIcon weight="duotone" className="text-sidebar-foreground" />
      <span>Calendar</span>
    </Link>
  )
}
