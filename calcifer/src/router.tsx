import { createRouter } from '@tanstack/react-router'
import { routeTree } from '~/routeTree.gen'

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
  interface HistoryState {
    // Set by create flows when they navigate to the new entity, so its page
    // focuses the title. Only on that history entry, never in the URL.
    justCreated?: boolean
  }
}
