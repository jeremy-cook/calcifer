# Phase 8 — Part 8: FE swap

Part of `app-plan.md` Phase 8. **No Rust here.** TypeScript-only sub-phase. Replaces the FE's localStorage persistence layer with RPC calls to the Rust server we just built.

## Goal

The FE talks to `:8080` for everything. Components don't change shape — selectors and hooks read from a TanStack Query cache instead of a Zustand-persisted map. The Watch stream feeds live updates into the cache for cross-tab sync.

## Files this part creates / modifies

```
calcifer/buf.gen.yaml                ← + connect-es plugin
calcifer/vite.config.ts              ← + /api proxy
calcifer/package.json                ← + connect, connect-web, react-query
calcifer/src/model/api.ts            ← NEW: clients, transports, hooks
calcifer/src/model/store.ts          ← REWRITTEN
calcifer/src/model/richtext.ts       ← REWRITTEN
calcifer/src/model/backlinks.ts      ← MODIFIED (derive from cache)
calcifer/src/model/linkSync.ts       ← DELETED (link/date derivation moved server-side to RichTextService.Put; see Part 6 amendment)
calcifer/src/routes/e.$id.tsx        ← MODIFIED (use new hooks)
calcifer/src/routes/s.$structureType.tsx
calcifer/src/components/sidebar/*    ← MODIFIED
calcifer/src/App.tsx                 ← + QueryClientProvider
```

## Steps

### 1. Regenerate proto with service stubs

`calcifer/buf.gen.yaml` — add the connect-es plugin:

```yaml
version: v2
plugins:
  - remote: buf.build/bufbuild/es:v2
    out: gen/ts
  - remote: buf.build/connectrpc/es:v2
    out: gen/ts
inputs:
  - directory: ../proto
```

```bash
cd calcifer
pnpm proto:gen
```

Now `gen/ts/calcifer/v1/services_connect.ts` exists with `EntityService` + `RichTextService` exports.

### 2. Add client deps

```bash
pnpm add @connectrpc/connect @connectrpc/connect-web @tanstack/react-query
```

### 3. Vite proxy

`calcifer/vite.config.ts`:

```ts
export default defineConfig({
  // ...
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        // strip /api prefix when forwarding:
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
})
```

Why proxy: avoids CORS in dev (request origin matches the page) and lets us point a single base URL at either the dev server or a production deployment later.

### 4. Build the clients

`calcifer/src/model/api.ts`:

```ts
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import { createPromiseClient } from '@connectrpc/connect'
import { EntityService, RichTextService } from '@calcifer/proto/calcifer/v1/services_connect'

const transport = createGrpcWebTransport({
  baseUrl: '/api',
})

export const entityClient = createPromiseClient(EntityService, transport)
export const richTextClient = createPromiseClient(RichTextService, transport)
```

The `connect-web` library's `createGrpcWebTransport` produces a transport that speaks gRPC-Web protocol — compatible with the `tonic-web` layer we wired in Part 7.

### 5. Wrap App in QueryClientProvider

`calcifer/src/App.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 1000 * 60 },  // entity list rarely changes per second
  },
})

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  )
}
```

Export `queryClient` so the Watch consumer (step 9) can call `setQueryData` directly.

### 6. Hooks

In `api.ts` (or split into `hooks.ts`):

```ts
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'

export function useEntities(structureType?: string) {
  return useQuery({
    queryKey: ['entities', structureType ?? 'all'],
    queryFn: async () => {
      const res = await entityClient.list({ structureType: structureType ?? '' })
      return res.entities
    },
  })
}

export function useEntity(id: string) {
  return useQuery({
    queryKey: ['entity', id],
    queryFn: () => entityClient.get({ id }),
    enabled: !!id,
  })
}

export function useCreateEntity() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (entity: Entity) => entityClient.create({ entity }),
    onSuccess: (created) => {
      qc.setQueryData(['entity', created.id], created)
      qc.invalidateQueries({ queryKey: ['entities'] })
    },
  })
}

// useUpdateEntity, useDeleteEntity, useRichText, usePutRichText follow
// the same shape.
```

For the editor's autosave, **`useUpdateEntity` should optimistically update**:

```ts
export function useUpdateEntity() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (entity: Entity) => entityClient.update({ entity }),
    onMutate: async (entity) => {
      await qc.cancelQueries({ queryKey: ['entity', entity.id] })
      const prev = qc.getQueryData<Entity>(['entity', entity.id])
      qc.setQueryData(['entity', entity.id], entity)
      return { prev }
    },
    onError: (_err, entity, ctx) => {
      if (ctx?.prev) qc.setQueryData(['entity', entity.id], ctx.prev)
    },
    onSettled: (_data, _err, entity) => {
      qc.invalidateQueries({ queryKey: ['entity', entity.id] })
    },
  })
}
```

### 7. Rewrite `model/store.ts`

The Zustand `persist` middleware goes away. The store keeps any pure-client state (sidebar collapse, toolbar UI state) but the `entities: Record<string, Entity>` map is now owned by TanStack Query. Selectors that scanned the map become memoized hooks that read from `useEntities()`:

```ts
// Before: listByStructure(entities, 'Note')
// After:
export function useNotesList() {
  const { data: entities = [] } = useEntities('Note')
  return useMemo(
    () => entities.toSorted((a, b) => Number(b.updatedAt) - Number(a.updatedAt)),
    [entities],
  )
}
```

Apply the same translation to `entitiesByDate`, `daysWithContent`, `dailyNoteByDate`.

### 8. Rewrite `model/richtext.ts`

```ts
export function useRichText(ref: RichTextRef) {
  return useQuery({
    queryKey: ['richtext', ref.entityId, ref.propertyId],
    queryFn: () => richTextClient.get(ref),
    enabled: !!ref.entityId,
  })
}

export function usePutRichText() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (rt: RichText) => richTextClient.put(rt),
    onSuccess: (saved) => {
      qc.setQueryData(['richtext', saved.ref!.entityId, saved.ref!.propertyId], saved)
    },
  })
}
```

Editor autosave uses `usePutRichText().mutate(...)` debounced just like before.

### 9. Watch consumer

Boot a long-lived watch loop once at app startup. `App.tsx`:

```tsx
useEffect(() => {
  let cancelled = false
  ;(async () => {
    while (!cancelled) {
      try {
        for await (const event of entityClient.watch({})) {
          if (cancelled) break
          if (event.event.case === 'upserted') {
            const e = event.event.value
            queryClient.setQueryData(['entity', e.id], e)
            queryClient.invalidateQueries({ queryKey: ['entities'] })
          } else if (event.event.case === 'deletedId') {
            queryClient.removeQueries({ queryKey: ['entity', event.event.value] })
            queryClient.invalidateQueries({ queryKey: ['entities'] })
          }
        }
      } catch (err) {
        if (cancelled) break
        console.warn('watch disconnected; reconnecting in 1s', err)
        await new Promise((r) => setTimeout(r, 1000))
      }
    }
  })()
  return () => { cancelled = true }
}, [])
```

Async iterator is the cleanest API; connect-web returns one for server-streaming RPCs.

### 10. Update consumers

- `routes/e.$id.tsx`: replace `useEntityStore(...)` with `useEntity(id)`; replace store update calls with mutation hooks.
- `routes/s.$structureType.tsx`: same pattern with `useEntities(type)`.
- `model/backlinks.ts`: derive from `useEntities()` data.
- `model/linkSync.ts`: **deleted.** Link/date derivation moves into `RichTextService.Put` server-side (Part 6 amendment), so the FE no longer walks the doc on save — `Put` does, atomically with the doc write. The autosave callback in `EntityRichTextField` drops its `syncLinksFromDoc(...)` call and just fires the `Put` mutation.
- Sidebar / `+ New` / `createDailyNote` / `moveDailyNote`: route through mutation hooks.

### 11. Document the dev loop

Add to root `README.md` (or create one):

```bash
# Terminal 1 — backend
cd server
cargo watch -x run

# Terminal 2 — frontend
cd calcifer
pnpm dev
```

## Verification (the Phase 8 acceptance test)

End-to-end through the browser:

1. Boot fresh: `rm server/calcifer.db && cd server && cargo run`; `pnpm dev` in `calcifer/`.
2. Create a Note via `+ New`; refresh; persists. `sqlite3 server/calcifer.db "SELECT id, name FROM entities;"` shows the row.
3. Type richtext content; wait for autosave; refresh; restored. `SELECT length(doc) FROM richtext;` non-zero.
4. `@`-mention another note; backlinks panel on the target shows the source.
5. Insert a date chip; calendar references panel for that day shows the source.
6. Two tabs: edit the name in tab A; tab B's sidebar updates within ~1s without manual refresh.
7. Delete an entity; cascade verified (`SELECT COUNT(*) FROM properties WHERE entity_id = '<id>';` returns 0).
8. Open the network panel: requests go to `/api/calcifer.v1.EntityService/...` with `content-type: application/grpc-web+proto`.

## Done when

All 8 verification steps pass; the FE has zero references to localStorage for entities or richtext.

## Phase 8 complete.

Next up: Phase 9 (embeddings / RAG substrate). Different problem space — vector indexing, background jobs, LLM provider integration.
