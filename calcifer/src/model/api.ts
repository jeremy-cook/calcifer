import { Code, ConnectError, createClient } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import { QueryClient } from '@tanstack/react-query'
import { EntityService, RichTextService, StructureService } from '@calcifer/proto/calcifer/v1/services_pb'

// gRPC-Web under /api: the backend serves it (and this app) in production, and
// the Vite dev server proxies it there.
const transport = createGrpcWebTransport({ baseUrl: '/api' })

export const entityClient = createClient(EntityService, transport)
export const richTextClient = createClient(RichTextService, transport)
export const structureClient = createClient(StructureService, transport)

// Single app-wide cache for requested data (structures, rich-text docs) and
// mutations. Exported so the Watch loop (`model/sync.ts`) and imperative
// (non-hook) call sites can read/update it directly. Entities live in
// `model/replica.ts` instead.
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, retry: false },
  },
})

export const qk = {
  structures: () => ['structures'] as const,
  richtext: (entityId: string, propertyId: string) => ['richtext', entityId, propertyId] as const,
  // Prefix of every `richtext(...)` key, for matching them all.
  everyRichtext: () => ['richtext'] as const,
}

export function isAlreadyExists(err: unknown): boolean {
  return err instanceof ConnectError && err.code === Code.AlreadyExists
}
