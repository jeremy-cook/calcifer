# T09 · Server write validation

**Area:** server · **Issues:** I-24 (A7), I-25 (A8), I-26 (C3, flags only), I-16

## Background
`EntityService` (`server/src/services/entity.rs`) writes through Create (by intent),
Rename, SetProperty, Delete and Resolve. There is no Update and no client-built Create.
The registry (`server/src/structures.rs`) declares `creatable`, `name_editable` and
`unique_names` for each structure, but the server enforces none of them. Your prompt
includes decision **D2** (name lookup rule). Recommended: deterministic, oldest
`created_at` wins, `id` breaks ties.

## Requirements
1. **Unknown structure types (I-24).** Every path that takes a `structure_type` (Create,
   Resolve by name, `List` when non-empty) returns `InvalidArgument` for a type not in
   the registry. `List` with `""` still means all.
2. **Deterministic lookup (I-24, D2).**
   - `find_by_name` orders by `created_at, id` (or whatever D2 says).
   - Document the rule in the `services.proto` comment on `Resolve`. This comment-only
     proto edit is allowed; regenerate both TS stubs.
   - Add a test with two same-named Notes.
3. **Unique-clash messages (I-25).**
   - `map_unique_violation` must report which constraint failed. A DailyNote date clash
     gives "a DailyNote for <date> already exists". A Tag name clash gives
     "a Tag named "<name>" already exists".
   - Find out what SQLite's error text contains for each index (column list or index
     name) and branch on that, with a comment.
   - Fix every call site, including `SetProperty(date)`, which passes a hard-coded
     DailyNote message today.
   - Test a Create and a Rename onto an existing Tag name.
4. **`name_editable` (I-26).** `Rename` of a structure with `name_editable = false`
   (DailyNote) returns `FailedPrecondition`.
5. **`creatable` (I-26).** Create of a non-creatable structure (DailyNote) returns
   `FailedPrecondition` with a message pointing at `Resolve`. `Resolve` by date still
   creates daily notes.
6. **Property kind check (I-16).**
   - Generalise the select check (`validate_select`, I-9) so that every **declared**
     property's value case must match its declared `PropertyKind`. Apply it in Create
     and SetProperty.
   - Rich-text-kind properties accept only the `richtext` case (until T12 removes it).
   - Undeclared property ids: keep today's behaviour and mention it in your report.
   - Tests: a `relations` value on `content`, and a `date` value on a select.
7. **Rename name hygiene (tech lead note from T08).** `Create` trims the name and treats
   a blank one as unset, but `Rename` stores whatever it's given. `Rename` trims too, and
   returns `InvalidArgument` for a blank name. Test both.

## Out of scope
Client changes. Rich-text addressing (T12). Unused kinds (T23).

## Done when
- All the above holds with tests, and the existing suite passes.
- `calcifer` builds and `mcp-server` typechecks with the regenerated stubs.
- `ISSUES.md`: move I-16, I-24 and I-25 to Resolved (house format). Add a line to I-26
  saying the flag half is done and the kinds half remains.
- `docs/reference/data-model.md` notes the enforced flags and the lookup rule.

## Commits
One `server:` commit per numbered requirement, e.g.
`server: reject unknown structure types (I-24)`. Then
`docs: resolve I-16, I-24 and I-25; note I-26 progress`.
