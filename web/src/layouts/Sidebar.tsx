import { CalendarItem } from '~/layouts/sidebar/CalendarItem'
import { HomeItem } from '~/layouts/sidebar/HomeItem'
import { NewButton } from '~/layouts/sidebar/NewButton'
import { StructureNav } from '~/layouts/sidebar/StructureNav'

export function Sidebar() {
  return (
    <nav className="flex h-full flex-col gap-0.5 overflow-y-auto bg-sidebar px-2 py-3 text-sm text-sidebar-foreground">
      <NewButton />
      <HomeItem />
      <CalendarItem />
      <div className="px-2.5 pt-4 pb-1 text-[0.7rem] font-bold tracking-widest text-muted-foreground uppercase">
        Library
      </div>
      <StructureNav />
    </nav>
  )
}
