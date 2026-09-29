// Calcifer MCP server (stdio). Exposes the knowledge base to an external agent
// (Claude Code) — search, read, create, link — all through the tonic core.
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'
import * as ops from './tools.js'

const text = (s: string) => ({ content: [{ type: 'text' as const, text: s }] })

// How note markdown refers to other entities; shared by the tools that read or write it.
const LINK_SYNTAX =
  '[[Name]] links a note (created if missing); [[Structure/Name]] links another structure from list_structures, e.g. [[Todo/Ship]] or [[DailyNote/June 13, 2026]] (a missing DailyNote stays plain text); #tag applies a tag. These become real graph links automatically.'

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
  `Read a note by name, returned as markdown, with the list of notes that link to it. Links are written with each target's current name: ${LINK_SYNTAX} Writing the markdown back unchanged links the same targets.`,
  { name: z.string() },
  async ({ name }) => text(await ops.getNote(name)),
)

server.tool(
  'create_note',
  `Create (or replace) a note. In the markdown body, ${LINK_SYNTAX} Search first to avoid duplicates.`,
  { name: z.string(), markdown: z.string() },
  async ({ name, markdown }) => text(await ops.createNote(name, markdown)),
)

server.tool(
  'append_to_note',
  `Append markdown to an existing note, growing it across turns. In the appended text, ${LINK_SYNTAX}`,
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
  `Append markdown to the journal entry for a calendar day (ISO date), creating it if needed. In the text, ${LINK_SYNTAX} ISO dates (e.g. 2026-06-13) become date references.`,
  { date: z.string(), markdown: z.string() },
  async ({ date, markdown }) => text(await ops.appendToDailyNote(date, markdown)),
)

server.tool(
  'list_structures',
  'List the entity types (structures) available in this knowledge base.',
  {},
  async () => text(await ops.listStructures()),
)

const transport = new StdioServerTransport()
await server.connect(transport)
console.error('calcifer MCP server running on stdio')
