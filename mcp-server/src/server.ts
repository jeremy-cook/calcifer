// Calcifer MCP server (stdio). Exposes the knowledge base to an external agent
// (Claude Code) — search, read, create, link — all through the tonic core.
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'
import * as ops from './tools.js'

const text = (s: string) => ({ content: [{ type: 'text' as const, text: s }] })

const server = new McpServer({ name: 'calcifer', version: '0.0.0' })

server.tool(
  'search_notes',
  'Search existing notes and tags. Use this before creating, to find and reuse existing notes instead of making duplicates. `mode` picks the ranking: "lexical" (keyword/FTS5), "semantic" (meaning via embeddings), or "hybrid" (both, fused — the default and best for natural-language queries).',
  {
    query: z.string(),
    limit: z.number().int().positive().optional(),
    mode: z.enum(['lexical', 'semantic', 'hybrid']).optional(),
  },
  async ({ query, limit, mode }) => text(await ops.searchNotes(query, limit, mode)),
)

server.tool(
  'get_note',
  'Read a note by name, returned as markdown, with the list of notes that link to it.',
  { name: z.string() },
  async ({ name }) => text(await ops.getNote(name)),
)

server.tool(
  'create_note',
  'Create (or replace) a note. The markdown body may reference other notes with [[Wikilinks]] and apply #tags; these become real graph links automatically. Search first to avoid duplicates.',
  { name: z.string(), markdown: z.string() },
  async ({ name, markdown }) => text(await ops.createNote(name, markdown)),
)

server.tool(
  'append_to_note',
  'Append markdown to an existing note, growing it across turns. [[Wikilinks]] and #tags in the appended text become links.',
  { name: z.string(), markdown: z.string() },
  async ({ name, markdown }) => text(await ops.appendToNote(name, markdown)),
)

server.tool(
  'link_notes',
  'Link one note to another by adding a [[wikilink]] reference to the first note.',
  { from: z.string(), to: z.string() },
  async ({ from, to }) => text(await ops.linkNotes(from, to)),
)

server.tool(
  'get_backlinks',
  'List the notes that link to a given note.',
  { name: z.string() },
  async ({ name }) => text(await ops.getBacklinks(name)),
)

server.tool(
  'create_daily_note',
  "Create the journal entry for a calendar day (ISO date, e.g. \"2026-06-13\"). One DailyNote exists per day; if it already exists this is a no-op that returns the existing note.",
  { date: z.string() },
  async ({ date }) => text(await ops.createDailyNote(date)),
)

server.tool(
  'append_to_daily_note',
  'Append markdown to the journal entry for a calendar day (ISO date), creating it if needed. [[Wikilinks]], #tags and dates in the text become graph links automatically.',
  { date: z.string(), markdown: z.string() },
  async ({ date, markdown }) => text(await ops.appendToDailyNote(date, markdown)),
)

server.tool(
  'list_structures',
  'List the entity types (structures) available in this knowledge base.',
  {},
  async () => text(ops.listStructures()),
)

const transport = new StdioServerTransport()
await server.connect(transport)
console.error('calcifer MCP server running on stdio')
