import { RouterProvider } from '@tanstack/react-router'
import { QueryClientProvider, useQuery } from '@tanstack/react-query'
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
// read it synchronously from the query cache.
function StructureRegistryGate() {
  const { isPending, isError } = useQuery(structuresQuery)
  if (isPending) return null
  if (isError) {
    return (
      <p className="p-8 text-sm text-muted-foreground">Couldn't load structures from the server. Reload to retry.</p>
    )
  }
  return <WatchedRouter />
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <StructureRegistryGate />
    </QueryClientProvider>
  )
}
