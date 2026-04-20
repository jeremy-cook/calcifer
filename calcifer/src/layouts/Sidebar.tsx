import { Separator } from '~/components/ui/separator'
import { CalendarItem } from '~/layouts/sidebar/CalendarItem'
import { NewButton } from '~/layouts/sidebar/NewButton'
import { StructureNav } from '~/layouts/sidebar/StructureNav'

export function Sidebar() {
  return (
    <nav className="flex h-full flex-col gap-2 overflow-y-auto bg-muted/40 p-3 text-sm">
      <NewButton />
      <Separator />
      <CalendarItem />
      <Separator />
      <StructureNav />
    </nav>
  )
}
