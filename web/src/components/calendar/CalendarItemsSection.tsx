import type { Icon } from '@phosphor-icons/react'
import { LoadingRows } from '~/components/loading/LoadingRows'
import type { CalendarItem } from '~/model/calendar'
import { useReplicaReady } from '~/model/store'
import { CalendarItemRow } from './CalendarItemRow'

interface CalendarItemsSectionProps {
  title: string
  icon: Icon
  color: string
  items: readonly CalendarItem[]
}

// A day's list under a small heading with its count; an empty one says so on that line,
// once the first snapshot has said what the day holds.
export function CalendarItemsSection({ title, icon: IconComponent, color, items }: CalendarItemsSectionProps) {
  const ready = useReplicaReady()

  const renderCount = () => {
    if (!ready) return null
    return (
      <span className="font-medium tracking-normal normal-case">· {items.length === 0 ? 'none' : items.length}</span>
    )
  }

  return (
    <section className="flex min-w-0 flex-col gap-1">
      <h2 className="flex h-6 items-center gap-2 text-xs font-bold tracking-wider text-muted-foreground uppercase">
        <IconComponent weight="duotone" className="size-4" style={{ color }} aria-hidden />
        {title}
        {renderCount()}
      </h2>
      {!ready && <LoadingRows count={2} />}
      {items.length > 0 && <List items={items} />}
    </section>
  )
}

interface ListProps {
  items: readonly CalendarItem[]
}

function List({ items }: ListProps) {
  return (
    <ul className="flex flex-col">
      {items.map((item) => (
        <CalendarItemRow key={`${item.kind}:${item.entity.id}`} item={item} />
      ))}
    </ul>
  )
}
