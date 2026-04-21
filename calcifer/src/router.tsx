import { Outlet, RouterProvider, createRootRoute, createRoute, createRouter } from '@tanstack/react-router'
import { TiptapEditor } from '~/editors/tiptap/TiptapEditor'
import { AppShell } from '~/layouts/AppShell'
import { CalendarPage } from '~/pages/CalendarPage'
import { StructureListPage } from '~/pages/StructureListPage'

function RootLayout() {
  return (
    <AppShell>
      <Outlet />
    </AppShell>
  )
}

function IndexPage() {
  return <TiptapEditor />
}

function EntityPage() {
  const { id } = entityRoute.useParams()
  return <div className="p-6 text-sm text-muted-foreground">Entity {id} — Phase 3</div>
}

function StructureListRouteView() {
  const { structureId } = structureListRoute.useParams()
  return <StructureListPage structureId={structureId} />
}

const rootRoute = createRootRoute({ component: RootLayout })

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: IndexPage,
})

const entityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/e/$id',
  component: EntityPage,
})

const calendarRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/calendar',
  component: CalendarPage,
})

const structureListRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/s/$structureId',
  component: StructureListRouteView,
})

const routeTree = rootRoute.addChildren([indexRoute, entityRoute, calendarRoute, structureListRoute])

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

export { RouterProvider }
