import { CalendarBlankIcon } from '@phosphor-icons/react'
import { Link } from '@tanstack/react-router'
import { NAV_ROW_ACTIVE_CLASS, NAV_ROW_CLASS } from './nav-row'

export function CalendarItem() {
  return (
    <Link to="/calendar" activeProps={{ className: NAV_ROW_ACTIVE_CLASS }} className={NAV_ROW_CLASS}>
      <CalendarBlankIcon weight="duotone" className="text-sidebar-foreground" />
      <span>Calendar</span>
    </Link>
  )
}
