# T23 · Unused property kinds

**Area:** proto, server, fe, mcp · **Issues:** I-26 (C3, the kinds half)

## Problem
No structure in `server/src/structures.rs` declares a `text`, `number` or (singular)
`relation` property. The proto still has `PropertyKind.TEXT/NUMBER/RELATION` and
`PropertyValue.text/number/relation`. The server carries code for them (e.g. relation
link sync in `link_store.rs`), and the FE silently renders nothing for them
(`routes/e.$id.tsx`, `renderProperty`).

Your prompt includes decision **D6**, and says whether the tech lead confirmed that the
live DB holds no values of these kinds.

## Variant "remove" (recommended)
- **Proto:** delete `PROPERTY_KIND_TEXT`, `PROPERTY_KIND_NUMBER` and
  `PROPERTY_KIND_RELATION` and add `reserved` for their numbers and names. Delete the
  `text`, `number` and `relation` cases from `PropertyValue` and reserve their numbers
  and names.
- **Server:** remove the code paths for them, including the singular-relation link sync,
  if `relations` doesn't share it. Keep T08's kind check exhaustive. Tests follow.
- **FE:** remove the dead switch cases and any helpers.
- **MCP:** remove them from `KIND_NAMES` in `tools.ts`.
- Regenerate both TS stubs.

## Variant "render"
- The FE renders an editor for each kind on the entity page: a text input, a number
  input, and a single-relation picker reusing `EntityRelationsField` patterns. Use the
  model write helpers from T21.
- No proto change.
- Add a temporary structure property in a scratch branch to check each one in the
  browser. Don't commit that.

## Done when
- Variant remove: `grep -rn "RELATION\b\|Relation(\|case: 'relation'\|case: 'text'\|case: 'number'" calcifer/src mcp-server/src server/src --exclude-dir=gen`
  shows only `relations` (plural) uses. `cargo test`, `pnpm build`, `pnpm lint` and the
  MCP `pnpm typecheck` pass.
- Variant render: every `PropertyKind` has an editor on the entity page. Report the
  browser checks.
- `docs/reference/data-model.md` is updated.
- `ISSUES.md`: move I-26 to Resolved.

## Commits
Variant remove: `proto: remove unused property kinds (I-26)` (merge with the server and
consumer fixes if needed to keep the build green), then `docs: resolve I-26`.
Variant render: `fe: render text, number and relation properties (I-26)`, then
`docs: resolve I-26`.
