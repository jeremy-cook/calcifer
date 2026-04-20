import { useCallback, useMemo } from 'react'
import type { ReactNode } from 'react'
import type { Layout } from 'react-resizable-panels'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '~/components/ui/resizable'
import { useSidebarState } from '~/hooks/useSidebarState'
import { Sidebar } from '~/layouts/Sidebar'
import { TopBar } from '~/layouts/TopBar'

export interface AppShellProps {
  children: ReactNode
}

interface AppBodyProps {
  collapsed: boolean
  children: ReactNode
}

const LAYOUT_STORAGE_KEY = 'calcifer.sidebar.layout'

function AppBody({ collapsed, children }: AppBodyProps) {
  const defaultLayout = useMemo<Layout | undefined>(() => {
    const raw = localStorage.getItem(LAYOUT_STORAGE_KEY)
    if (!raw) return undefined
    try {
      return JSON.parse(raw) as Layout
    } catch {
      return undefined
    }
  }, [])

  const onLayoutChanged = useCallback((layout: Layout) => {
    localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(layout))
  }, [])

  if (collapsed) {
    return <main className="flex-1 overflow-auto">{children}</main>
  }
  const groupProps = { orientation: 'horizontal', defaultLayout, onLayoutChanged, className: 'flex-1' } as const
  return (
    <ResizablePanelGroup {...groupProps}>
      <ResizablePanel id="sidebar" defaultSize="280px" minSize="240px" maxSize="360px">
        <Sidebar />
      </ResizablePanel>
      <ResizableHandle />
      <ResizablePanel id="main" defaultSize="100%" minSize="200px">
        <main className="h-full overflow-auto">{children}</main>
      </ResizablePanel>
    </ResizablePanelGroup>
  )
}

export function AppShell({ children }: AppShellProps) {
  const { collapsed, toggle } = useSidebarState()

  return (
    <div className="flex h-screen flex-col">
      <TopBar onToggleSidebar={toggle} />
      <AppBody collapsed={collapsed}>{children}</AppBody>
    </div>
  )
}
