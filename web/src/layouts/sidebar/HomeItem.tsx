import { HouseIcon } from '@phosphor-icons/react'
import { Link } from '@tanstack/react-router'
import { cn } from '~/lib/utils'
import { NAV_ROW_ACTIVE_CLASS, NAV_ROW_CLASS, NAV_ROW_RAIL_CLASS } from './nav-row'

export interface HomeItemProps {
  rail: boolean
}

export function HomeItem({ rail }: HomeItemProps) {
  return (
    <Link
      to="/"
      title={rail ? 'Home' : undefined}
      activeOptions={{ exact: true }}
      activeProps={{ className: NAV_ROW_ACTIVE_CLASS }}
      className={cn(NAV_ROW_CLASS, rail && NAV_ROW_RAIL_CLASS)}
    >
      <HouseIcon weight="duotone" className="text-sidebar-foreground" />
      <span>Home</span>
    </Link>
  )
}
