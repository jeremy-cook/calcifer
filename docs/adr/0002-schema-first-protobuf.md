# 2. Author the data model in protobuf, generate both languages

- **Status:** Accepted
- **Date:** 2026-04-01 (adopted well before a server existed; `85e9f7f`)

## Context

[ADR 1](0001-polymorphic-entity-model.md) makes `PropertyValue` a sum type over text,
number, date, select, relation, and rich-text-pointer. Hand-written, that type drifts:
TypeScript reaches for `Record<string, unknown>` or a hand-rolled union that nothing
validates, and the eventual server reimplements the same shape independently.

The communication layer was always going to be RPC. The question was *when* to
introduce the schema — at the point a server existed (the natural moment), or before,
while the store was still browser-local.

Deferring meant writing the TS types twice: once by hand for the localStorage store,
then again as generated code when the server landed, with a rewrite of every consumer
in between.

## Decision

Author the data model once in `proto/calcifer/v1/entities.proto` and generate into
both languages: TS via `buf` (aliased `@calcifer/proto`), Rust via `tonic-build` at
compile time. **Adopt it immediately**, while the store is still localStorage-backed —
the local store uses proto-generated types from day one.

Model `PropertyValue` as a proto `oneof`, which `@bufbuild/protobuf` generates as a
discriminated union, so the sum type is exhaustively checked in TS.

## Consequences

- The backend swap was a storage change, not a type rewrite. The FE's types did not
  move when localStorage was replaced by gRPC-Web.
- Client and server cannot disagree about the model; there is one authored source.
- Exhaustive `switch` over `PropertyValue.value.case` in TS, with the compiler
  catching every unhandled variant when a new property type is added.
- Editing the schema means regenerating for **two** TS consumers separately
  (`calcifer/` and `mcp-server/`), because each keeps its own generated stubs. This is
  a known wart; consolidating it is a roadmap item. *(Updated 2026-09-29: the two
  consumers used to pin different `@bufbuild/protobuf` versions. Both now pin
  `@bufbuild/protobuf`, `@bufbuild/protoc-gen-es` and `@bufbuild/buf` to the same exact
  versions and generate with a local plugin, so only the separate stubs remain.)*
- Proto naming leaks into both languages: `structure_type` becomes `structureType` in
  TS, and the `ref` field becomes `r#ref` in Rust.
- Generated code is committed for TS (`calcifer/gen/ts`) but gitignored for
  `mcp-server`, so a fresh `mcp-server` checkout needs `pnpm proto:gen` before it
  typechecks.
