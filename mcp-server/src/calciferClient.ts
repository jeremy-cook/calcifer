import { createClient } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-node'
import { EntityService, RichTextService, SearchService } from '@calcifer/proto/calcifer/v1/services_pb'

// Reuse the same gRPC-Web endpoint the browser uses (tonic-web on :8080).
// From Node there's no CORS/proxy concern, so we point straight at the server.
const transport = createGrpcWebTransport({
  baseUrl: 'http://localhost:8080',
  httpVersion: '1.1',
})

export const entityClient = createClient(EntityService, transport)
export const richTextClient = createClient(RichTextService, transport)
export const searchClient = createClient(SearchService, transport)
