# T07 · New write RPCs: Create by intent, Rename, Resolve (additive)

**Area:** proto, server · **Issues:** I-20 (A1), I-21 (A2), I-23 (A4), I-15

## Background
Read ADR 8 in `docs/adr/` first: clients send intent and the server builds entities.
This task adds the new write surface **next to** the old one, so the frontend and MCP
keep working unchanged. Later tasks move the clients over, and T12 removes the old RPCs.

Today (`server/src/services/entity.rs`):
- `Create` takes a client-built `Entity` and persists its `links` and `referenced_dates`
  (`persist_new_entity`), which violates ADR 3.
- Defaults are built in `build_resolved_entity` and `build_daily_note` (and separately
  on the FE).
- `SetProperty(date)` on a DailyNote updates `date_key` but not the name.
- There's no Rename; renames go through `Update`, which replaces every property (I-15).

## Proto changes (`services.proto`)
Additive. Existing generated code must keep compiling.
- `CreateEntityRequest`:
  - Keep `Entity entity = 1`, commented `// Deprecated: removed in T12. Use the fields
    below.`
  - Add `string structure_type = 2; optional string name = 3; repeated Property
    properties = 4;` with comments: the server mints the id, timestamps, default
    properties and default name, and a DailyNote's name is always derived from its
    `date`.
- `rpc Rename(RenameEntityRequest) returns (Entity);` with
  `message RenameEntityRequest { string id = 1; string name = 2; }`. It changes only the
  name.
- `rpc Resolve(ResolveEntityRequest) returns (ResolveEntityResponse);`
  ```proto
  message ResolveEntityRequest {
    string structure_type = 1;          // required for `name`; must be "" or "DailyNote" for `date`
    oneof key {
      string name = 2;                  // case-insensitive
      string date = 3;                  // ISO yyyy-MM-dd; finds the day's DailyNote
    }
    bool create_if_missing = 4;         // false and missing => NOT_FOUND
  }
  message ResolveEntityResponse { Entity entity = 1; bool created = 2; }
  ```
- Comment `ResolveByName` and `CreateDailyNote` as deprecated in favour of `Resolve`, and
  `Update` in favour of `Rename` plus `SetProperty`.
- Message names follow the house `…EntityRequest` style; T19 aligns RPC names.

Regenerate both TS stubs.

## Server changes
- **One default builder:** replace `build_resolved_entity` and `build_daily_note` with a
  single `new_entity(structure_type, name: Option<&str>, properties)`. It:
  - mints a uuid;
  - adds rich-text property values (still `RichTextRef`, which the FE needs until T11 and
    T12);
  - adds select defaults;
  - overlays the caller's properties (the caller wins on the same id; reject a caller
    value for a rich-text property with `InvalidArgument`);
  - picks the name.
- **Default name:** `Untitled <StructureDef.name>`. For a `unique_names` structure (Tag),
  if that name is taken (case-insensitive), use `Untitled Tag 2`, `Untitled Tag 3`, and
  so on. (Today a second "+ New Tag" fails with the wrong message.)
- **DailyNote name rule:** one helper sets `name = format_long_date(date)` for a DailyNote.
  Apply it on every write path: Create (both forms), Update (legacy), `SetProperty` when
  `property_id == "date"`, and Resolve. `SetProperty(date)` must update the name, the
  FTS name row and `date_key` in the same transaction.
- **Create:**
  - With `entity` set (legacy): keep the client id, but **ignore** client `links`,
    `referenced_dates`, `created_at` and `updated_at`.
  - Otherwise: use `new_entity`.
  - Both publish Upserted as today.
- **Rename:**
  - Update `name`, the FTS name and `updated_at` only; publish Upserted.
  - A missing id is `NotFound`.
  - A unique clash maps through `map_unique_violation` (its message is fixed in T08).
- **Resolve:**
  - The name key reuses the ResolveByName path.
  - The date key looks up by `date_key` and creates via `new_entity("DailyNote", …)`.
  - A concurrent create that loses the unique race must re-read and return the winner
    with `created = false`, not error.
  - Reimplement `resolve_by_name` and `create_daily_note` on top of the same internals so
    there's one code path.

## Tests
At least:
- `create_by_intent_mints_id_defaults_and_name`
- `create_legacy_ignores_client_links_and_dates`
- `create_second_untitled_tag_gets_a_free_name`
- `rename_changes_only_the_name`
- `rename_concurrent_with_set_property_keeps_both`: this is I-15's done-when
- `set_property_date_renames_daily_note`
- `resolve_by_date_gets_or_creates`
- `resolve_by_date_without_create_is_not_found`
- `resolve_by_name_gets_or_creates`

## Out of scope
- Rejecting unknown or non-creatable types, `name_editable`, lookup order, error
  messages, kind checks (T08).
- Client changes (T09–T11).
- Removing anything (T12).

## Done when
- The new RPCs work as above, the old ones still work, and all tests pass.
- `calcifer` builds and `mcp-server` typechecks without source changes.
- `docs/reference/data-model.md` documents Create-by-intent, Rename and Resolve, and
  marks the old RPCs deprecated.

## Commits
1. `proto: add Create-by-intent, Rename and Resolve (I-20, I-23)`, with regenerated stubs.
2. `server: build entities and daily-note names server-side (I-20, I-21, I-23)`
3. `server: implement Rename and Resolve (I-15, I-23)`
4. `docs: document Create-by-intent, Rename and Resolve (I-20, I-23)`
