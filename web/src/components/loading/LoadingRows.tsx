import { cn } from '~/lib/utils'
import { useShownAfterDelay } from './use-shown-after'

export interface LoadingRowsProps {
  count?: number
  className?: string
}

// Name and date widths, varied so the rows don't read as a grid.
const WIDTHS = [
  [62, 14],
  [48, 16],
  [70, 12],
  [40, 15],
  [55, 14],
  [66, 13],
  [45, 16],
] as const

// Faint zebra rows in a list's place until its data arrives.
export function LoadingRows({ count = 7, className }: LoadingRowsProps) {
  const shown = useShownAfterDelay()
  if (!shown) return null

  return (
    <ul role="status" aria-label="Loading" className={cn('flex flex-col', className)}>
      {Array.from({ length: count }, (_, i) => {
        const [name, date] = WIDTHS[i % WIDTHS.length]
        return (
          <li key={i} className="flex h-[30px] items-center gap-2.5 rounded-[5px] px-2.5 even:bg-card/45">
            <span className="loading-bar size-4 shrink-0 rounded-[4px]" />
            <span className="loading-bar h-2.5" style={{ width: `${name}%` }} />
            <span className="loading-bar ml-auto h-2.5" style={{ width: `${date}%` }} />
          </li>
        )
      })}
    </ul>
  )
}
