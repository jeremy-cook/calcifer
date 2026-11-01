import { RouterProvider } from '@tanstack/react-router'
import { QueryClientProvider, useQuery } from '@tanstack/react-query'
import { ServerWaiting } from '~/components/ServerWaiting'
import { router } from '~/router'
import { queryClient } from '~/model/api'
import { structuresQuery } from '~/model/structures'
import { useEntitySync } from '~/model/sync'

function WatchedRouter() {
  useEntitySync()
  return <RouterProvider router={router} />
}

// The structure registry comes from the server. Nothing under the router
// renders until it has loaded, so routes, the editor and store helpers can
// read it synchronously from the query cache. The gate waits, retrying, until
// the server answers, and says so once an attempt has failed.
function StructureRegistryGate() {
  const { isPending, failureCount } = useQuery(structuresQuery)
  if (isPending) return failureCount > 0 ? <ServerWaiting /> : null
  return <WatchedRouter />
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <StructureRegistryGate />
    </QueryClientProvider>
  )
}
