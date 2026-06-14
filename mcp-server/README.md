# Calcifer MCP server

A stdio [Model Context Protocol](https://modelcontextprotocol.io) server that gives an
agent (e.g. Claude Code) knowledge-base tools backed by the Calcifer core. Notes the agent
writes are persisted by the Rust backend and show up **live** in the open browser tab via the
`Watch` stream — so the agent can do research and you review it in the note app.

## Prerequisites

The backend must be running (the MCP server talks to it over gRPC-web on `http://localhost:8080`):

```bash
cd server && cargo run          # backend on :8080 (auto-creates server/calcifer.db)
cd calcifer && pnpm dev         # frontend on http://localhost:5173 (to review notes live)
```

> **Node version trap:** fresh shells may resolve Node 16 and break `pnpm`/`tsx`. Prefix node
> commands with `export PATH="/Users/bebop/.nvm/versions/node/v24.12.0/bin:$PATH"`. The Claude Code
> registration below pins the v24 binary explicitly so it is unaffected.

## Run standalone

```bash
cd mcp-server && pnpm start      # tsx src/server.ts — serves MCP over stdio
```

## Register with Claude Code

Registered at **local** scope (private to you, this project). Run from the repo root:

```bash
claude mcp add calcifer -- sh -c \
  'cd /Users/bebop/Github/calcifer/mcp-server && exec /Users/bebop/.nvm/versions/node/v24.12.0/bin/node --import tsx src/server.ts'
```

The `sh -c` wrapper `cd`s into this package (so `tsx` and `src/server.ts` resolve regardless of the
spawn cwd) and uses the pinned Node v24 binary. Verify:

```bash
claude mcp get calcifer          # → Status: ✓ Connected
```

After registering, **restart Claude Code** so the `calcifer` tools load into the session. Remove with
`claude mcp remove calcifer -s local`.

## Tools

| tool | purpose |
|---|---|
| `search_notes` | find existing notes/tags; `mode` = `lexical` \| `semantic` \| `hybrid` (default) |
| `get_note` | read a note as markdown + its backlinks |
| `create_note` | create/replace a note; `[[Wikilinks]]` and `#tags` become real graph links |
| `append_to_note` | grow a note across turns |
| `link_notes` | add a `[[wikilink]]` from one note to another |
| `get_backlinks` | list notes linking to a given note |
| `create_daily_note` | create the journal entry for an ISO date (one per day) |
| `append_to_daily_note` | append to a day's journal, creating it if needed |
| `list_structures` | list entity types (structures) in the knowledge base |

## Verify the ops directly (no Claude Code needed)

```bash
cd mcp-server && pnpm test:tools   # backend must be up; exercises every op end-to-end
```
