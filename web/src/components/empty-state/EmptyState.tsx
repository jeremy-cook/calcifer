import type { Icon } from '@phosphor-icons/react'

export interface EmptyStateProps {
  icon: Icon
  color: string
  title: string
  description: React.ReactNode
  // A button or link that gets the reader out ("New note", "Clear filters").
  action?: React.ReactNode
}

// A whole page with nothing on it: the icon in the logo's rings, a line, and a way out.
export function EmptyState({ icon: IconComponent, color, title, description, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center gap-2 py-16 text-center">
      <div className="hearth-rings mb-1.5 flex size-21 items-center justify-center">
        <IconComponent weight="duotone" className="size-9" style={{ color }} aria-hidden />
      </div>
      <h2 className="font-heading text-xl font-semibold text-heading">{title}</h2>
      <p className="max-w-xs text-sm text-muted-foreground">{description}</p>
      {action && <div className="mt-2">{action}</div>}
    </div>
  )
}
