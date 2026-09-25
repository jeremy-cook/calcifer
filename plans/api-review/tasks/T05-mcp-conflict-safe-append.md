# T05 · MCP: conflict-safe appends

**Area:** mcp · **Issues:** I-17 (B1), I-18 (B2)

## Background
The server now supports `RichText.expected_updated_at` on Put:
- unset means an unconditional write;
- set means `FAILED_PRECONDITION` unless the stored doc's `updated_at` matches;
- the epoch means "nothing saved yet".

`RichTextService.Get` returns an empty `doc` with epoch `updated_at` for a declared
property that has nothing saved, and `NOT_FOUND` only for a missing entity. See the
comments in `proto/calcifer/v1/entities.proto` and `services.proto`.

## Problem
`mcp-server/src/tools.ts`: `appendToNote` and `appendToDailyNote` read the doc, merge,
and write back unconditionally. A browser save between the read and the write loses one
side. `getDoc` turns `NotFound` into an empty doc.

## Requirements
- `getDoc` returns `{ doc, updatedAt }` and relies on the new Get semantics. Remove the
  `NotFound` special case, and `isNotFound` if nothing else uses it.
- Factor the append logic into one helper used by both append tools. Read the doc,
  merge, then Put with `expectedUpdatedAt` set to the `updatedAt` it read. On
  `Code.FailedPrecondition`, re-read and retry, up to 3 attempts in total. After that,
  throw an error that says the note kept changing.
- `createNote` stays unconditional. Its intent is to overwrite. Add a one-line comment
  saying so.
- Update `src/tools-test.ts` if its expectations change. Don't run it.

## Out of scope
Moving daily-note resolution to `Resolve` (T07). `ListBacklinks` (T18). Search (T17).

## Done when
- Both append tools send `expectedUpdatedAt` and retry on `FailedPrecondition`.
- No `NotFound` handling remains for "not saved yet".
- `pnpm typecheck` passes.
- Your report tells the tech lead how to exercise the retry: e.g. append via MCP while a
  browser tab with the same note open saves.

## Commit
`mcp: send expected_updated_at on appends and retry on conflict (I-17, I-18)`
