# T19 · buf naming and lint

**Area:** proto, server, fe, mcp · **Issues:** I-13

## Problem
`buf lint` (`cd calcifer && pnpm proto:lint`) fails on the default RPC rules: request and
response messages aren't named `<Rpc>Request` / `<Rpc>Response`, and messages
(`Entity`, `RichText`, `EntityRef`, …) are reused as the request or response of several
RPCs. Nothing runs lint today. Read the I-13 entry in `ISSUES.md`.

Your prompt includes decision **D7**, and the task has two variants.

## Variant A: adopt buf's conventions (recommended)
- **RPC names:** use verb and noun on every service, so the existing `…EntityRequest`
  messages already fit:
  - EntityService: `GetEntity`, `ListEntities`, `CreateEntity`, `RenameEntity`,
    `SetEntityProperty`, `DeleteEntity`, `ResolveEntity`, `ListBacklinks`,
    `WatchEntities`;
  - `RichTextService.GetRichText` and `PutRichText`;
  - `SearchService.Search`;
  - `StructureService.ListStructures`.

  Rename the request messages to match (e.g. `SetPropertyRequest` →
  `SetEntityPropertyRequest`).
- **Responses:** wrap every response in `<Rpc>Response`, e.g.
  `GetEntityResponse { Entity entity = 1; }`, `WatchEntitiesResponse` (the old
  `EntityEvent`, same fields and numbers), `DeleteEntityResponse {}`,
  `PutRichTextResponse { RichText rich_text = 1; }`.
- **Requests:** `GetRichTextRequest { string entity_id; string property_id; }` and
  `PutRichTextRequest { string entity_id; string property_id; string doc;
  google.protobuf.Timestamp expected_updated_at; }`. Remove `expected_updated_at` from
  `RichText`. Remove `RichTextRef`, `EntityRef` or `EntityRefList` if nothing uses them
  any more (relations still do use `EntityRef`).
- Update the server, FE and MCP for the new generated names and the response unwrapping.
  Keep this mechanical, with no behaviour changes.
- `buf.yaml` stays on the default lint rules. Add a CI-less guard: document
  `pnpm proto:lint` in the README's dev loop under "after editing anything under
  `proto/`".

## Variant B: keep the names and document the exceptions
- `buf.yaml`: add `except` for `RPC_REQUEST_STANDARD_NAME`, `RPC_RESPONSE_STANDARD_NAME`
  and `RPC_REQUEST_RESPONSE_UNIQUE`, with a comment linking the convention.
- Write the convention down at the top of `services.proto`.
- Fix any remaining lint findings.

## Both variants
- `pnpm proto:lint` exits 0.
- Regenerate both TS stubs. `cargo test`, `pnpm build`, `pnpm lint` and the MCP
  `pnpm typecheck` all pass.
- `docs/reference/data-model.md` uses the new names.
- `ISSUES.md`: move I-13 to Resolved.

## Out of scope
Any behaviour change. Re-ordering field numbers: wire numbers stay the same wherever a
message keeps its role.

## Commits
Variant A: `proto: adopt buf RPC naming (I-13)`, with the stubs and all consumer updates
in one atomic commit unless you can keep smaller commits building. Then
`docs: resolve I-13`.
Variant B: `proto: document naming convention and configure buf lint (I-13)`, then
`docs: resolve I-13`.
