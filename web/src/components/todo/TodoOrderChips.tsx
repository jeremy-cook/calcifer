import { Fragment } from 'react'
import { ArrowDownIcon, ArrowUpIcon } from '@phosphor-icons/react'
import { cn } from '~/lib/utils'
import { TODO_ORDER_OPTIONS, type TodoOrder, type TodoOrderKey, type TodoSearch } from '~/model/todos'

export interface TodoOrderChipsProps {
  order: TodoOrder
  onChange: (next: Partial<TodoSearch>) => void
}

// The order within each group, read left to right. Clicking a chip moves it to the
// front; clicking the front one flips its direction.
export function TodoOrderChips({ order, onChange }: TodoOrderChipsProps) {
  const directions: Record<TodoOrderKey, [string, string]> = {
    priority: ['high first', 'low first'],
    due: ['soonest first', 'latest first'],
    name: ['A–Z', 'Z–A'],
  }
  const labelOf = (key: TodoOrderKey) => TODO_ORDER_OPTIONS.find((o) => o.key === key)?.label ?? key

  const handleClick = (key: TodoOrderKey, i: number) =>
    onChange(
      i === 0
        ? { reversed: order.reversed ? undefined : true }
        : { order: [key, ...order.keys.filter((k) => k !== key)], reversed: undefined },
    )

  return (
    <div className="flex items-center gap-1 text-xs text-muted-foreground">
      <span className="mr-1">Order</span>
      {order.keys.map((key, i) => {
        const first = i === 0
        const Arrow = order.reversed ? ArrowUpIcon : ArrowDownIcon
        const title = first
          ? `${labelOf(key)}, ${directions[key][order.reversed ? 1 : 0]}. Click to flip.`
          : `Order by ${labelOf(key)} first`
        return (
          <Fragment key={key}>
            {!first && <span aria-hidden>›</span>}
            <button
              type="button"
              title={title}
              aria-label={title}
              onClick={() => handleClick(key, i)}
              className={cn(
                'flex h-7 items-center gap-1 rounded-full border border-border px-2.5 font-semibold text-secondary-foreground hover:bg-accent',
                first && 'border-primary/70 bg-primary/10 text-accent-foreground',
                i === order.keys.length - 1 && 'font-medium text-muted-foreground',
              )}
            >
              {labelOf(key)}
              {first && <Arrow weight="bold" className="size-3" />}
            </button>
          </Fragment>
        )
      })}
    </div>
  )
}
