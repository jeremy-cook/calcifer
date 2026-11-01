import type { Icon } from '@phosphor-icons/react'
import { cn } from '~/lib/utils'

export interface EmptyLineProps {
  icon: Icon
  color: string
  children: React.ReactNode
  className?: string
}

// A section or list with nothing in it: one quiet line, led by the icon.
export function EmptyLine({ icon: IconComponent, color, children, className }: EmptyLineProps) {
  return (
    <p className={cn('flex items-start gap-2.5 px-2.5 py-1 text-sm text-muted-foreground', className)}>
      <IconComponent weight="duotone" className="mt-0.5 size-4 shrink-0" style={{ color }} aria-hidden />
      <span>{children}</span>
    </p>
  )
}
