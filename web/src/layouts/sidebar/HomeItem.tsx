import { HouseIcon } from '@phosphor-icons/react'
import { Link } from '@tanstack/react-router'
import { NAV_ROW_ACTIVE_CLASS, NAV_ROW_CLASS } from './nav-row'

export function HomeItem() {
  return (
    <Link
      to="/"
      activeOptions={{ exact: true }}
      activeProps={{ className: NAV_ROW_ACTIVE_CLASS }}
      className={NAV_ROW_CLASS}
    >
      <HouseIcon weight="duotone" className="text-sidebar-foreground" />
      <span>Home</span>
    </Link>
  )
}
