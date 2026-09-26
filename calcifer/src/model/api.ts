import { Code, ConnectError, createClient } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import { QueryClient } from '@tanstack/react-query'
import { EntityService, RichTextService, StructureService } from '@calcifer/proto/calcifer/v1/services_pb'

// gRPC-Web over the Vite /api proxy → tonic-web on :8080.
const transport = createGrpcWebTransport({ baseUrl: '/api' })

export const entityClient = createClient(EntityService, transport)
export const richTextClient = createClient(RichTextService, transport)
export const structureClient = createClient(StructureService, transport)

// Single app-wide cache. Exported so the Watch replica (`model/sync.ts`) and
// imperative (non-hook) call sites can read/update it directly.
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, retry: false },
  },
})

export const qk = {
  structures: () => ['structures'] as const,
  entities: () => ['entities', 'all'] as const,
  entity: (id: string) => ['entity', id] as const,
  // Prefix of every `entity(id)` key, for matching them all.
  everyEntity: () => ['entity'] as const,
  richtext: (entityId: string, propertyId: string) => ['richtext', entityId, propertyId] as const,
  // Prefix of every `richtext(...)` key, for matching them all.
  everyRichtext: () => ['richtext'] as const,
}

export function isNotFound(err: unknown): boolean {
  return err instanceof ConnectError && err.code === Code.NotFound
}
