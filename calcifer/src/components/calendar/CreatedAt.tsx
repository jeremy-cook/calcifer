import { CommandIcon } from '@phosphor-icons/react'

export function CreatedAt() {
  return (
    <div className="flex items-center gap-2 text-sm text-muted-foreground">
      <CommandIcon className="size-4" />
      <span>Created on This Day (0)</span>
    </div>
  )
}
