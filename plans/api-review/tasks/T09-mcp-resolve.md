# T09 · MCP: move to Resolve

**Area:** mcp · **Issues:** I-23 (A4)

## Background
`EntityService.Resolve(ResolveEntityRequest)` → `ResolveEntityResponse { entity,
created }` replaces `ResolveByName` and `CreateDailyNote`. See the comments in
`proto/calcifer/v1/services.proto`:
- key `name` (with `structure_type`) or key `date` (ISO; implies DailyNote);
- `create_if_missing = false` and missing gives `NOT_FOUND`.

The old RPCs still exist, but are deprecated and removed in T12.

## Problem
`mcp-server/src/tools.ts`:
- the `resolver` and `getNote` use `resolveByName`;
- `resolveDailyNote` calls `createDailyNote`, and on `AlreadyExists` lists every
  DailyNote and scans their properties;
- `src/markdown/verify.ts` also calls `resolveByName`.

## Requirements
- Replace every `resolveByName` call with `resolve({ structureType, key: { case: 'name',
  value } , createIfMissing })`. Check the generated oneof shape in `src/gen/`.
- `resolveDailyNote(date)` becomes a single `resolve` call with the `date` key and
  `createIfMissing: true`. Delete the fallback scan and the `AlreadyExists` handling.
- `createDailyNote`'s message can use `created` to say "created" or "already existed".
- Update `src/tools-test.ts` expectations if they change. Don't run it.
- Nothing in `mcp-server/src` (outside `gen/`) references `resolveByName` or
  `createDailyNote` RPCs afterwards. The MCP *tool* named `create_daily_note` stays.

## Out of scope
Append conflict handling (already done). Search (T17). ListBacklinks (T18).

## Done when
- `grep -rn "resolveByName\|entityClient.createDailyNote" mcp-server/src --exclude-dir=gen`
  finds nothing.
- `pnpm typecheck` passes.

## Commit
`mcp: resolve notes and daily notes through Resolve (I-23)`
