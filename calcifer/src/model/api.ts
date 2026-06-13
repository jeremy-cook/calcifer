import { Code, ConnectError, createClient } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import { QueryClient } from '@tanstack/react-query'
import { EntityService, RichTextService } from '@calcifer/proto/calcifer/v1/services_pb'

// gRPC-Web over the Vite /api proxy → tonic-web on :8080.
const transport = createGrpcWebTransport({ baseUrl: '/api' })

export const entityClient = createClient(EntityService, transport)
export const richTextClient = createClient(RichTextService, transport)

// Single app-wide cache. Exported so the Watch consumer and imperative
// (non-hook) call sites can read/update it directly.
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, retry: false },
  },
})

export const qk = {
  entities: () => ['entities', 'all'] as const,
  entity: (id: string) => ['entity', id] as const,
  richtext: (entityId: string, propertyId: string) => ['richtext', entityId, propertyId] as const,
}

export function isNotFound(err: unknown): boolean {
  return err instanceof ConnectError && err.code === Code.NotFound
}
