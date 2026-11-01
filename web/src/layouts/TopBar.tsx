import { SidebarSimpleIcon } from '@phosphor-icons/react'
import { Button } from '~/components/ui/button'

export interface TopBarProps {
  onToggleSidebar: () => void
}

export function TopBar({ onToggleSidebar }: TopBarProps) {
  return (
    <header className="flex flex-col">
      <div className="flex h-11 items-center gap-2 px-2">
        <Button
          variant="ghost"
          size="icon"
          onClick={onToggleSidebar}
          aria-label="Toggle sidebar"
          className="text-muted-foreground"
        >
          <SidebarSimpleIcon />
        </Button>
        <div className="hearth-logo size-5 shrink-0" aria-hidden />
        <div className="font-heading text-base font-bold">Calcifer</div>
      </div>
      <div className="hearth-stripe h-[5px]" aria-hidden />
    </header>
  )
}
