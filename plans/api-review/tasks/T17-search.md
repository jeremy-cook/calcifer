# T17 · One Search RPC with a mode, and match ranges

**Area:** proto, server, mcp · **Issues:** I-29 (A5)

## Problem
`SearchService` has `Search(query, limit)` (lexical FTS5) and `Retrieve(query, k,
hybrid)` (vector or hybrid, falling back to lexical when embeddings are off). Both return
`SearchResponse`. The MCP tool (`mcp-server/src/tools.ts`, `searchNotes`) already models
one call with `mode: 'lexical' | 'semantic' | 'hybrid'`. Snippets mark matches with
`[`…`]` (`server/src/services/search.rs`, the `snippet(entity_fts, -1, '[', ']', …)`
call), which clashes with `[[wikilink]]` syntax. The frontend doesn't call search.

## Requirements
- **Proto:**
  ```proto
  enum SearchMode {
    SEARCH_MODE_UNSPECIFIED = 0;   // server default: document which one (match the MCP tool's default)
    SEARCH_MODE_LEXICAL = 1;
    SEARCH_MODE_SEMANTIC = 2;
    SEARCH_MODE_HYBRID = 3;
  }
  message SearchRequest { string query = 1; uint32 limit = 2; SearchMode mode = 3; }
  message MatchRange { uint32 start = 1; uint32 end = 2; }   // half-open, UTF-16 code units into snippet
  message SearchHit { Entity entity = 1; string snippet = 2; double score = 3; repeated MatchRange matches = 4; }
  ```
  - Remove `rpc Retrieve` and `RetrieveRequest`.
  - Document the fallback: semantic or hybrid with embeddings off behaves as lexical.
  - Document that `snippet` is plain text.
- **Server:**
  - One `search` handler dispatches on mode, reusing the existing lexical, vector and
    RRF code.
  - The snippet is produced with markers that can't occur in note text (e.g. private-use
    code points), then the markers are stripped and converted to `MatchRange`s in UTF-16
    code units.
  - Semantic hits with no lexical match have an empty `matches`.
  - Tests: mode dispatch, a range test with non-ASCII text before the match, and
    snippets containing no `[`/`]` markers.
- **MCP:** `searchNotes` makes one `search` call with the mode enum. Render matches in
  the tool's text output with a marker that doesn't clash with wikilinks (e.g. `**…**`).
- Regenerate both TS stubs. `calcifer` must still build.

Tech lead note (from T13): `hydrate` in `server/src/services/search.rs` loads each hit
with `load_entity`, so it costs about 4 queries per hit. While you're rewriting the
search path, hydrate the hits with a batch load instead (extend T13's `load_entities`
with an id-set filter, or add a sibling next to it). Output must not change.

## Out of scope
Search ranking changes. FE search UI.

## Done when
- `Retrieve` is gone, and the tests above pass with `cargo test`.
- `mcp-server` typechecks and `calcifer` builds.
- `docs/reference/data-model.md` is updated.
- `ISSUES.md`: move I-29 to Resolved.

## Commits
1. `proto: merge Search and Retrieve and return match ranges (I-29)`
2. `server: one Search handler with modes and match ranges (I-29)`
3. `mcp: search through the single Search RPC (I-29)`
4. `docs: resolve I-29`

If the proto commit alone would break the server build, merge commits 1 and 2.
