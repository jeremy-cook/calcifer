# T12 · Remove the old write surface and stored RichTextRefs

**Area:** proto, server, docs · **Issues:** closes I-15, I-20, I-21, I-22, I-23

## Background
Both clients now use Create-by-intent, Rename, Resolve and SetProperty, and the FE
addresses rich text by declared property (ADR 8). This task deletes what they replaced.
**Gate G2:** your prompt must say the user approved decision **D5** (deleting stored
rich-text property rows). If it doesn't, stop and report.

## Proto
- Remove `rpc Update`, `rpc ResolveByName`, `rpc CreateDailyNote` and their messages
  (`UpdateEntityRequest`, `ResolveByNameRequest`, `ResolveByNameResponse`,
  `CreateDailyNoteRequest`).
- `CreateEntityRequest`: remove `entity = 1`, and add `reserved 1; reserved "entity";`.
- `PropertyValue`: remove `RichTextRef richtext = 6`, and add `reserved 6; reserved
  "richtext";`. `RichTextRef` stays as `RichTextService`'s address.
- Mark `Entity`'s server-owned fields (`id`, `links`, `referenced_dates`, `created_at`,
  `updated_at`) with an "output only" comment.
- Regenerate both TS stubs. Fix any compile fallout in `calcifer/` and `mcp-server/`,
  which should be none. If there's more than trivial fallout, stop and report.

## Server
- Delete the `update`, `resolve_by_name` and `create_daily_note` handlers and the
  legacy-Create branch.
- `creatable` is enforced on every Create (remove T08's `// T12:` note).
- `new_entity` no longer writes rich-text property values.
- `SetProperty` and Create reject any value for a rich-text-kind property.
- Tests:
  - rewrite the tests that used `Update` to use `Rename` or `SetProperty`, or delete
    them;
  - delete `update_from_a_stale_snapshot_loses_a_concurrent_edit`, since that behaviour
    is gone;
  - keep coverage for everything I-2, I-3, I-8, I-9 and I-11 fixed.
- **Migration** (`server/migrations/<timestamp>_drop_richtext_property_values.sql`):
  delete the `properties` rows that hold rich-text refs. `value_blob` is prost-encoded,
  so select rows by the registry instead: `(structure_type, property_id)` pairs whose
  kind is RICHTEXT (today `content` on Note, DailyNote and Todo; confirm in
  `structures.rs`). Join `entities`. Add a comment naming the source of the list.
- Build and test against a scratch DB, as the engineer brief describes for migrations.
  Never run it against `server/calcifer.db`; the tech lead does that after a backup.

## Docs
- `docs/reference/data-model.md`: remove the deleted RPCs and the `richtext` value case,
  and describe addressing by declared property.
- `ISSUES.md`: move I-15, I-20, I-21, I-22 and I-23 to Resolved (house format, one line
  each).

## Out of scope
Watch (T14). Relation dead refs (I-14). Unused kinds (T23).

## Done when
- `grep -rn "resolveByName\|createDailyNote\|UpdateEntityRequest\|case: 'richtext'" calcifer/src mcp-server/src server/src proto --exclude-dir=gen`
  finds nothing.
- `cargo test` passes against the scratch DB, `calcifer` builds and lints, and
  `mcp-server` typechecks.
- Your report gives the tech lead the migration's exact SQL and a read-only query to
  count the rows it will delete from the live DB.

## Commits
1. `proto: remove Update, ResolveByName, CreateDailyNote and stored RichTextRefs (I-20, I-22, I-23)`
2. `server: drop the legacy write paths (I-15, I-20, I-23)`
3. `server: migrate away stored rich-text property values (I-22)`
4. `docs: resolve I-15, I-20, I-21, I-22 and I-23`
