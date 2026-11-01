import { cn } from '~/lib/utils'
import { useShownAfterDelay } from './use-shown-after'

export interface LoadingTextProps {
  className?: string
}

// Line widths in percent; 0 is a paragraph break.
const LINES = [92, 86, 95, 60, 0, 88, 74]

// Faint lines of text in a note's place until it arrives.
export function LoadingText({ className }: LoadingTextProps) {
  const shown = useShownAfterDelay()
  if (!shown) return null

  return (
    <div role="status" aria-label="Loading" className={cn('flex max-w-3xl flex-col gap-3.5', className)}>
      {LINES.map((width, i) =>
        width === 0 ? (
          <div key={i} className="h-2" />
        ) : (
          <div key={i} className="loading-bar h-3" style={{ width: `${width}%` }} />
        ),
      )}
    </div>
  )
}
