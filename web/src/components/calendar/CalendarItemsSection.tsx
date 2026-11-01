import type { Icon } from '@phosphor-icons/react'
import type { CalendarItem } from '~/model/calendar'
import { CalendarItemRow } from './CalendarItemRow'

interface CalendarItemsSectionProps {
  title: string
  icon: Icon
  color: string
  emptyText: string
  items: readonly CalendarItem[]
}

export function CalendarItemsSection({
  title,
  icon: IconComponent,
  color,
  emptyText,
  items,
}: CalendarItemsSectionProps) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="flex items-center gap-2 text-sm font-bold">
        <IconComponent weight="duotone" className="size-[18px]" style={{ color }} />
        {title}
      </h2>
      {items.length === 0 ? <EmptyState text={emptyText} /> : <List items={items} />}
    </section>
  )
}

interface EmptyStateProps {
  text: string
}

function EmptyState({ text }: EmptyStateProps) {
  return <p className="text-sm text-muted-foreground">{text}</p>
}

interface ListProps {
  items: readonly CalendarItem[]
}

function List({ items }: ListProps) {
  return (
    <ul className="flex flex-col gap-0.5">
      {items.map((item) => (
        <CalendarItemRow key={`${item.kind}:${item.entity.id}`} item={item} />
      ))}
    </ul>
  )
}
