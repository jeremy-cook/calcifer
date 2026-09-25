# T07 · Resolve replaces ResolveByName and CreateDailyNote; server-owned daily-note names

**Area:** proto, server, fe, mcp · **Issues:** I-23 (A4), I-21 (A2, the server half)

## Background
Read ADR 8 in `docs/adr/` first: clients send intent and the server builds entities.

Proto changes in this plan **break in place** (decision D4): one task changes the proto
and every consumer together, and deletes what it replaces. Nothing is left deprecated.
Intermediate commits inside this task may leave a consumer uncompiled, but the branch
must build, lint, typecheck and pass tests when you report.

Today:
- **Server** (`server/src/services/entity.rs`): `ResolveByName` and `CreateDailyNote`
  are separate RPCs. Defaults are built in `build_resolved_entity` and
  `build_daily_note`. `SetProperty(date)` on a DailyNote updates `date_key` but not the
  name.
- **MCP** (`mcp-server/src/`): `tools.ts` calls `resolveByName` in the resolver (about
  l.12) and `getNote` (about l.62 and l.98). `resolveDailyNote` calls `createDailyNote`
  (about l.115) and, on `AlreadyExists`, lists every DailyNote and scans their
  properties. `markdown/verify.ts` (about l.58) also calls `resolveByName`.
- **FE** (`calcifer/src/`): `model/store.ts` resolves mentions and wikilinks with
  `resolveByName` (about l.298). `useCreateDailyNote` builds a DailyNote with
  `buildDailyNoteMessage` and sends `Create({ entity })`.
  `components/calendar/DailyNoteDateField.tsx` moves a daily note with
  `useUpdateEntity(withDailyNoteDate)`.

## Proto changes (`services.proto`)
- Add:
  ```proto
  rpc Resolve(ResolveEntityRequest) returns (ResolveEntityResponse);

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
- **Remove** `rpc ResolveByName`, `rpc CreateDailyNote`, `ResolveByNameRequest`,
  `ResolveByNameResponse` and `CreateDailyNoteRequest`.
- Comment `SetProperty`: for a DailyNote, setting `date` also sets the name, and fails
  with `ALREADY_EXISTS` if that day already has a note.
- Message names follow the house `…EntityRequest` style; T19 aligns RPC names.

Regenerate both TS stubs.

## Server changes
- **One default builder:** replace `build_resolved_entity` and `build_daily_note` with a
  single `new_entity(structure_type, name: Option<&str>, properties)`. It:
  - mints a uuid;
  - adds rich-text property values (still `RichTextRef`, which the FE needs until T12);
  - adds select defaults;
  - overlays the caller's properties (the caller wins on the same id; reject a caller
    value for a rich-text property with `InvalidArgument`);
  - picks the name: the given one, or for a DailyNote the one derived from its date.
    (T08 adds the `Untitled <Structure>` default for Create.)
- **DailyNote name rule:** one helper sets `name = format_long_date(date)` for a DailyNote.
  Apply it on every write path that exists today: Create, Update, `SetProperty` when
  `property_id == "date"`, and Resolve. `SetProperty(date)` must update the name, the
  FTS name row and `date_key` in the same transaction.
- **Resolve:**
  - The name key takes over the old ResolveByName path.
  - The date key looks up by `date_key` and creates via `new_entity("DailyNote", …)`.
  - A concurrent create that loses the unique race must re-read and return the winner
    with `created = false`, not error.
  - Publishes Upserted when it creates.
- **Delete** the `resolve_by_name` and `create_daily_note` handlers. Port their tests to
  `Resolve`; don't drop coverage.

## FE changes
- The mention and wikilink resolve helper uses `Resolve` with the `name` key.
- `useCreateDailyNote(iso)` becomes a `Resolve` with the `date` key and
  `createIfMissing: true`. Delete `buildDailyNoteMessage`.
- `DailyNoteDateField` moves a note with the existing `useSetProperty` for `date`. On
  `ALREADY_EXISTS`, show the existing inline error. The new name comes from the server
  response. Delete `withDailyNoteDate`.
- Follow `CLAUDE.md` component style.

## MCP changes
- Replace every `resolveByName` call with `resolve({ structureType, key: { case: 'name',
  value }, createIfMissing })`. Check the generated oneof shape in `src/gen/`.
- `resolveDailyNote(date)` becomes a single `resolve` call with the `date` key and
  `createIfMissing: true`. Delete the fallback scan and the `AlreadyExists` handling.
- The `create_daily_note` tool's message uses `created` to say "created" or "already
  existed". The MCP *tool* and `ops.createDailyNote` keep their names.
- Update `src/tools-test.ts` expectations if they change. Don't run it.

## Tests
At least:
- `set_property_date_renames_daily_note`
- `set_property_date_onto_existing_day_is_already_exists`
- `resolve_by_date_gets_or_creates`
- `resolve_by_date_without_create_is_not_found`
- `resolve_by_name_gets_or_creates`
- `resolve_by_name_without_create_is_not_found`

## Out of scope
- Create-by-intent, Rename, and removing `Update` or `Create({ entity })` (T08). Leave
  `useCreateEntity`, `useUpdateEntity` and the title rename alone.
- Rejecting unknown or non-creatable types, `name_editable`, lookup order, error
  messages, kind checks (T09).
- How the editor finds its rich-text document (T12).
- `['entities']` invalidations (T15). Proto helpers leaking into components (T21).

## Done when
- `grep -rn "resolveByName\|ResolveByName\|CreateDailyNote\|entityClient.createDailyNote\|resolve_by_name\|fn create_daily_note\|buildDailyNoteMessage\|withDailyNoteDate" proto calcifer/src mcp-server/src server/src --exclude-dir=gen`
  finds nothing.
- `cargo test` passes, `calcifer` builds and lints, and `mcp-server` typechecks.
- `docs/reference/data-model.md` documents Resolve and the DailyNote name rule, and no
  longer lists the deleted RPCs.
- `ISSUES.md`: move I-23 to Resolved (house format). Add a line to I-21 saying the
  server now builds defaults in one place and the FE builders remain (T08).
- Browser checks to report:
  1. Opening a day with no note from the calendar creates it; opening it again reuses it.
  2. Changing a daily note's date makes its title follow. Moving it onto a day that has
     a note shows the inline error.
  3. Typing `[[New thing]]` and `#newtag` in a note still creates and links them.

## Commits
1. `proto: replace ResolveByName and CreateDailyNote with Resolve (I-23)`, with
   regenerated stubs.
2. `server: build new entities in one place and derive daily-note names (I-21, I-23)`
3. `server: implement Resolve (I-23)`
4. `fe: resolve entities and move daily notes through Resolve and SetProperty (I-23)`
5. `mcp: resolve notes and daily notes through Resolve (I-23)`
6. `docs: document Resolve; resolve I-23 (I-21, I-23)`
