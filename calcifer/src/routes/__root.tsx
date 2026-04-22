import { createRootRoute, Outlet } from '@tanstack/react-router'
import { AppShell } from '~/layouts/AppShell'

export const Route = createRootRoute({
  component: () => (
    <AppShell>
      <Outlet />
    </AppShell>
  ),
})
