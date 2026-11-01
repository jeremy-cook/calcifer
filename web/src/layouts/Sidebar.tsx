import { cn } from '~/lib/utils'
import { CalendarItem } from '~/layouts/sidebar/CalendarItem'
import { HomeItem } from '~/layouts/sidebar/HomeItem'
import { NewButton } from '~/layouts/sidebar/NewButton'
import { StructureNav } from '~/layouts/sidebar/StructureNav'

export interface SidebarProps {
  // Icons only, with names on hover: the sidebar in a narrow window.
  rail?: boolean
}

export function Sidebar({ rail = false }: SidebarProps) {
  const renderSection = () =>
    rail ? (
      <div className="mx-2 my-2 h-px bg-border" aria-hidden />
    ) : (
      <div className="px-2.5 pt-4 pb-1 text-[0.7rem] font-bold tracking-widest text-muted-foreground uppercase">
        Library
      </div>
    )

  return (
    <nav
      className={cn(
        'flex h-full flex-col gap-0.5 overflow-y-auto bg-sidebar px-2 py-3 text-sm text-sidebar-foreground',
        rail && 'w-13 shrink-0 px-1.5',
      )}
    >
      <NewButton rail={rail} />
      <HomeItem rail={rail} />
      <CalendarItem rail={rail} />
      {renderSection()}
      <StructureNav rail={rail} />
    </nav>
  )
}
