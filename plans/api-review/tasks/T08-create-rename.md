# T08 · Create by intent and Rename replace Create({ entity }) and Update

**Area:** proto, server, fe, mcp · **Issues:** I-20 (A1), I-21 (A2), I-15

## Background
Read ADR 8 in `docs/adr/` first: clients send intent and the server builds entities.

Proto changes in this plan **break in place** (decision D4): one task changes the proto
and every consumer together, and deletes what it replaces. Nothing is left deprecated.
Intermediate commits inside this task may leave a consumer uncompiled, but the branch
must build, lint, typecheck and pass tests when you report.

Already done: `Resolve` replaced `ResolveByName` and `CreateDailyNote`. The server builds
new entities in one place (`new_entity` in `server/src/services/entity.rs`), and a
DailyNote's name is derived from its date on every write.

Today:
- `Create` takes a client-built `Entity` and persists its `links` and `referenced_dates`
  (`persist_new_entity`), which violates ADR 3.
- There's no Rename. Renames go through `Update`, which replaces every property (I-15).
- **FE** (`calcifer/src/`):
  - `model/store.ts`: `defaultNameFor` (about l.21), `buildEntityMessage`,
    `useCreateEntity`, `useUpdateEntity`, `withName`.
  - `layouts/sidebar/NewButton.tsx` and `components/todo/TodoQuickAdd.tsx` create.
  - `routes/e.$id.tsx` renames the title via `useUpdateEntity(withName)`.
    `isDefaultName` (about l.166) decides title autofocus by comparing against the
    default name string.
- **MCP** (`mcp-server/src/`): `smoke.ts` (about l.12) and `markdown/verify.ts` (about
  l.65) call `create({ entity })` with a client-chosen id.

## Proto changes (`services.proto`)
- `CreateEntityRequest` becomes:
  ```proto
  message CreateEntityRequest {
    reserved 1;
    reserved "entity";
    string structure_type = 2;
    optional string name = 3;
    repeated Property properties = 4;
  }
  ```
  with comments: the server mints the id, timestamps, default properties and default
  name, and a DailyNote's name is always derived from its `date`.
- Add `rpc Rename(RenameEntityRequest) returns (Entity);` with
  `message RenameEntityRequest { string id = 1; string name = 2; }`. It changes only the
  name.
- **Remove** `rpc Update` and `UpdateEntityRequest`.
- Mark `Entity`'s server-owned fields (`id`, `links`, `referenced_dates`, `created_at`,
  `updated_at`) with an "output only" comment.

Regenerate both TS stubs.

## Server changes
- **Create** builds with `new_entity`. Client-supplied links and dates can no longer
  reach the server.
- **Default name:** extend `new_entity`'s name choice with `Untitled <StructureDef.name>`
  when no name is given (DailyNote keeps its date-derived name). For a `unique_names`
  structure (Tag), if that name is taken (case-insensitive), use `Untitled Tag 2`,
  `Untitled Tag 3`, and so on. (Today a second "+ New Tag" fails with the wrong message.)
- **Rename:**
  - Update `name`, the FTS name and `updated_at` only; publish Upserted.
  - A missing id is `NotFound`.
  - A unique clash maps through `map_unique_violation` (its message is fixed in T09).
- **Delete** the `update` handler and the old Create path. Remove the DailyNote name
  helper's Update call site.
- Tests that used `Update`:
  - rewrite them to use `Rename` or `SetProperty`, or delete them;
  - delete `update_from_a_stale_snapshot_loses_a_concurrent_edit`, since that behaviour
    is gone;
  - keep coverage for everything I-2, I-3, I-8, I-9 and I-11 fixed.

## FE changes
- **Create:** `useCreateEntity(structureType, name?)` calls Create by intent.
  `buildEntityMessage` and `defaultNameFor` go.
- **Rename:** add `useRenameEntity()`.
  - Optimistic on `name` only, in both the list cache and the entity cache.
  - Roll back only `name` on error, following the per-entity rollback pattern from I-6.
  - The title input uses it, and keeps its existing debounce/flush behaviour (I-1).
- **Title autofocus:** replace the `isDefaultName` string comparison with an explicit
  "just created" signal from the create flows. Recommended: router history state, e.g.
  `navigate({ …, state: { justCreated: true } })`, read in the entity route. A search
  param is acceptable if state is awkward with TanStack Router's types.
- **Delete** `useUpdateEntity`, `withName` and any helper left without callers.
- Follow `CLAUDE.md` component style.

## MCP changes
- `smoke.ts` and `markdown/verify.ts` create by intent and use the id the server
  returns. Don't run either.

## Tests
At least:
- `create_by_intent_mints_id_defaults_and_name`
- `create_second_untitled_tag_gets_a_free_name`
- `rename_changes_only_the_name`
- `rename_missing_id_is_not_found`
- `rename_concurrent_with_set_property_keeps_both`: this is I-15's done-when

## Out of scope
- Rejecting unknown or non-creatable types, `name_editable`, lookup order, error
  messages, kind checks (T09).
- How the editor finds its rich-text document (T12). Keep writing and reading the
  `richtext` property value for now.
- `['entities']` invalidations (T15). Proto helpers leaking into components (T21).

## Done when
- `grep -rn "entityClient.update\|UpdateEntityRequest\|rpc Update\|buildEntityMessage\|defaultName\|useUpdateEntity\|withName\|isDefaultName" proto calcifer/src mcp-server/src server/src --exclude-dir=gen`
  finds nothing, and no `create(` call in `calcifer/src` or `mcp-server/src` passes
  `entity`.
- `cargo test` passes, `calcifer` builds and lints, and `mcp-server` typechecks.
- `docs/reference/data-model.md` documents Create by intent and Rename, and no longer
  lists Update.
- `ISSUES.md`: move I-15, I-20 and I-21 to Resolved (house format, one line each).
- Browser checks to report:
  1. "+ New" for each creatable structure lands on a focused title.
  2. Two "+ New Tag" in a row both succeed.
  3. Rename a Todo while setting its status via MCP or a second tab; both persist.
  4. Opening an existing entity doesn't focus its title.

## Commits
1. `proto: replace Update and client-built Create with Rename and Create by intent (I-20, I-15)`,
   with regenerated stubs.
2. `server: create by intent and rename (I-15, I-20, I-21)`
3. `fe: create by intent and rename through Rename (I-15, I-20, I-21)`
4. `mcp: create by intent (I-20)`
5. `docs: document Create and Rename; resolve I-15, I-20 and I-21`
