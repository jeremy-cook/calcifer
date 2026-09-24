# 7. The server owns the structure registry; clients fetch it

- **Status:** Accepted
- **Date:** 2026-09-24 (resolves [I-10](../../ISSUES.md))

## Context

[ADR 1](0001-polymorphic-entity-model.md) differentiates entities by Structure, but the
structures themselves (types, property ids and kinds, select options and defaults,
relation targets, create/mention/naming rules) were hand-copied into three places:
`calcifer/src/model/structures.ts`, `server/src/structures.rs` and
`mcp-server/src/tools.ts`. Each new structure or option meant three edits that drifted:
the Rust copy knew only select *defaults*, the MCP copy was prose, and only the frontend
had option lists and labels.

The server now has to *enforce* the schema: relation targets (I-2) and select options
(I-9). That makes the Rust copy load-bearing, so it can't stay a partial mirror.

Two places could hold the one authored copy:

- **In proto**, as data (custom options or a checked-in textproto). Protobuf is good at
  shapes and poor at data: custom options are awkward to read at runtime in all three
  languages, and a textproto still needs a loader on each side.
- **In a Rust table**, served over an RPC. The server already needs the data in Rust to
  validate writes, and every client already talks to the server.

## Decision

**The registry is a static table in `server/src/structures.rs`. It is the only place
structure data is authored.** Proto defines the *shape* of the registry
(`StructureDef`, `PropertyDef`, `SelectOption`, and a `PropertyKind` enum) in
`proto/calcifer/v1/structures.proto`, and a new service exposes it:

```proto
service StructureService {
  rpc List(ListStructuresRequest) returns (ListStructuresResponse);
}
```

`List` takes no filters and returns every structure in a stable order. A `StructureDef`
carries: `type`, `name`, `plural`, `description` (one line, for the agent),
`properties`, `creatable`, `mentionable`, `unique_names`, `name_editable`. A
`PropertyDef` carries: `id`, `kind`, `label`, `options` (for select), `default_option`
(empty = none), `target_structure` (for relation/relations; empty = any).

The existing Rust helpers (`richtext_properties`, `select_defaults`,
`relation_properties`) are derived from the table, and the same table backs server-side
validation.

**What each client keeps:**

- **Frontend:** only presentation, keyed by structure type: icon and color, plus
  `nameMeta.derive` (a function, so it can't cross the wire) and per-option presentation
  such as badge variants. Each presentation map has a fallback so an unknown type or
  option still renders. The registry is fetched once through TanStack Query
  (`staleTime: Infinity`), and the app doesn't render the router until it has loaded, so
  synchronous accessors (`isMentionable`, the editor's mention code, route search
  validation) can read it from the query cache. Option lists and labels are no longer in
  the frontend. Keys the code branches on (`status = done`, `priority = none`) remain
  literals in the code that uses them; they are behaviour, not schema.
- **MCP server:** `list_structures` calls `StructureService.List` and formats the result.
  It holds no structure data.

## Consequences

- Adding a select option, or changing a label or default, is a one-file change in
  `structures.rs`. Adding a structure is `structures.rs` plus an optional icon/color entry
  in the frontend (the fallback covers it otherwise).
- The frontend loses compile-time types for option keys (`TodoStatus` and `TodoPriority`
  become `string`). Unknown values still fall back to the declared default.
- The app needs one extra round trip before first render. The registry is a few hundred
  bytes and never changes while the server runs.
- Changing the registry requires a server restart. Clients pick it up on next load.
- Structure data is no longer visible by reading the frontend. `structures.rs` is the
  place to look.
