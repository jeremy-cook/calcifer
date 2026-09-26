# T18 · ListBacklinks request/response and the self-link rule

**Area:** proto, server, mcp · **Issues:** I-30 (A6)

## Problem
- `rpc ListBacklinks(EntityRef) returns (ListEntitiesResponse)` reads only `id`. The MCP
  server fills `structureType: ''`.
- It returns bare entities, with no information on which property linked or when.
- The FE computes backlinks itself (`calcifer/src/model/backlinks.ts`) and **excludes**
  an entity linking to itself. The server **includes** it. The two disagree.
- The FE doesn't call this RPC; it's for the agent (see the Watch comment in
  `services.proto`).

## Requirements
- **Proto:**
  ```proto
  message ListBacklinksRequest { string entity_id = 1; }
  message Backlink {
    Entity source = 1;                          // the entity whose content/relations link here
    string source_property_id = 2;              // which property the link came from
    google.protobuf.Timestamp created_at = 3;   // when that link was first derived
  }
  message ListBacklinksResponse { repeated Backlink backlinks = 1; }
  ```
  - One `Backlink` per link row. Ordered newest `created_at` first, with `id` breaking
    ties; document that.
  - Document that self-links are excluded and that the RPC is intended for the agent.
- **Server:**
  - Implement the above. Exclude `source.id == entity_id`.
  - A missing target entity returns `NotFound`.
  - Hydrate sources with `load_entities` or a batch query, not N `load_entity` calls.
  - Tests: self-link excluded, property id and timestamp populated, missing target is
    NotFound.
- **MCP:** `backlinkNames` uses the new request. The tool output may show the property
  where useful; keep it short.
- **FE:** no behaviour change. Add a one-line comment in `backlinks.ts` noting that the
  self-link rule matches the server's.
- Regenerate both TS stubs.

Tech lead note (from T13): `list_backlinks` in `server/src/services/entity.rs` loads each
source with `load_entity` (1 + 4N queries). Load them with a batch load instead, sharing
whatever id-set loader T17 added (or adding one next to `load_entities`).

## Out of scope
Moving the FE onto this RPC. Relation dead refs (I-14).

## Done when
- The tests pass, `mcp-server` typechecks, and `calcifer` builds.
- `docs/reference/data-model.md` is updated.
- `ISSUES.md`: move I-30 to Resolved.

## Commits
`proto:`/`server:`/`mcp:`/`docs:` as in the engineer brief (merge proto and server if
needed to keep the build green), all tagged `(I-30)`.
