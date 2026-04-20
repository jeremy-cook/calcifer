import { SidebarSimpleIcon } from '@phosphor-icons/react'
import { Button } from '~/components/ui/button'

export interface TopBarProps {
  onToggleSidebar: () => void
}

export function TopBar({ onToggleSidebar }: TopBarProps) {
  return (
    <header className="flex h-10 items-center gap-2 border-b px-2">
      <Button variant="ghost" size="icon" onClick={onToggleSidebar} aria-label="Toggle sidebar">
        <SidebarSimpleIcon />
      </Button>
      <div className="text-sm text-muted-foreground">Calcifer</div>
    </header>
  )
}
