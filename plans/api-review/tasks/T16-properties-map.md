# T16 · Properties as a map

**Area:** proto, server, fe, mcp · **Issues:** I-28 (C1)

## Problem
`Entity.properties` is `repeated Property` (`Property { string id = 1; PropertyValue
value = 2; }`). Every consumer repeats `properties.find(p => p.id === id)?.value?.value`,
and nothing stops duplicate ids. A protobuf map entry is encoded exactly as `{key = 1,
value = 2}`, so `map<string, PropertyValue>` is **wire-compatible** and breaks only
source code.

## Requirements
- **Proto:** `Entity.properties` and `CreateEntityRequest.properties` become
  `map<string, PropertyValue> properties = <same number>;`. If `Property` is then unused,
  delete it. Regenerate both TS stubs.
- **Server:** follow the type change (prost generates a `HashMap<String,
  PropertyValue>`). Storage doesn't change, since property rows are already keyed.
  Anything that relied on property order must not. Check tests that compare
  `properties` vectors.
- **FE:** replace every `find` over `properties` with keyed access. If a small helper
  like `propertyValue(entity, id)` in `model/` reads better at the call sites, add it,
  but don't wrap what's already a one-liner. Optimistic-update code that copies
  `properties` must copy the object, not mutate it.
- **MCP:** the same (e.g. `tools.ts`, around the old daily-note scan, if any remains, and
  `describe…` helpers).
- All three consumers change in this one task, so the branch never has a mismatched
  proto.

## Out of scope
Renaming messages or RPCs (T19). Any behaviour change.

## Done when
- `grep -rn "properties.find\|properties.some\|properties.filter" calcifer/src mcp-server/src server/src --exclude-dir=gen`
  finds nothing, or only justified uses, which you list.
- `cargo test`, `pnpm build`, `pnpm lint` and `pnpm typecheck` (mcp) all pass.
- `docs/reference/data-model.md` shows the map.
- `ISSUES.md`: move I-28 to Resolved.

## Commits
1. `proto: make Entity.properties a map (I-28)`, with the regenerated stubs **and** the
   server/FE/MCP source updates. It's one atomic source break, so one commit is
   acceptable here; split it if you can keep every commit building.
2. `docs: resolve I-28 in ISSUES.md`
