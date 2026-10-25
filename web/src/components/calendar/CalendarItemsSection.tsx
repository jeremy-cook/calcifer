import type { CalendarItem } from '~/model/calendar'
import { CalendarItemRow } from './CalendarItemRow'

interface CalendarItemsSectionProps {
  title: string
  emptyText: string
  items: readonly CalendarItem[]
}

export function CalendarItemsSection({ title, emptyText, items }: CalendarItemsSectionProps) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-base font-semibold">{title}</h2>
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
