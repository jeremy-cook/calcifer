import { useEffect } from 'react'
import { RouterProvider } from '@tanstack/react-router'
import { QueryClientProvider, useQuery } from '@tanstack/react-query'
import { router } from '~/router'
import { entityClient, qk, queryClient } from '~/model/api'
import { structuresQuery } from '~/model/structures'

// Long-lived Watch loop: server EntityEvents feed the cache so any open tab
// (and the MCP agent's writes) reflect live without polling. Also warms the
// global entity list so synchronous reads (mention search, calendar) work.
function useWatchSync() {
  useEffect(() => {
    let cancelled = false
    void queryClient.prefetchQuery({
      queryKey: qk.entities(),
      queryFn: async () => (await entityClient.list({ structureType: '' })).entities,
    })
    void (async () => {
      while (!cancelled) {
        try {
          for await (const event of entityClient.watch({})) {
            if (cancelled) break
            if (event.event.case === 'upserted') {
              const e = event.event.value
              queryClient.setQueryData(qk.entity(e.id), e)
              void queryClient.invalidateQueries({ queryKey: ['entities'] })
            } else if (event.event.case === 'deletedId') {
              queryClient.removeQueries({ queryKey: qk.entity(event.event.value) })
              void queryClient.invalidateQueries({ queryKey: ['entities'] })
            }
          }
        } catch (err) {
          if (cancelled) break
          console.warn('watch disconnected; reconnecting in 1s', err)
          await new Promise((r) => setTimeout(r, 1000))
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])
}

function WatchedRouter() {
  useWatchSync()
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
