# T04 · RichText Put: validate, conflict-check, publish events; Get returns empty

**Area:** proto, server · **Issues:** I-17 (B1), I-18 (B2)

## Problem
`server/src/services/richtext.rs`:
- `Put` publishes nothing to the watch hub, so other tabs never hear about a new
  document, links or `referenced_dates`.
- `Put` has no conflict check. The browser's debounced save overwrites an agent's
  append.
- `Put` accepts any `(entity_id, property_id)`, even a missing entity or an undeclared
  property, so orphaned documents are possible.
- `Get` returns `NotFound` for "declared but nothing saved yet", which every client has
  to special-case.

## Proto changes (`proto/calcifer/v1/`)
Additive only. Existing consumers must keep compiling.
1. `RichText` gains `google.protobuf.Timestamp expected_updated_at = 4;` with this
   comment:
   - Put only; ignored on output.
   - Unset means an unconditional write.
   - Set means the write fails with `FAILED_PRECONDITION` unless the stored document's
     `updated_at` equals it exactly.
   - The epoch (0 seconds, 0 nanos) means "expect nothing saved yet".
   - Clients echo back the `updated_at` they last read.

   (Keeping `RichText` as Put's request type is deliberate; T19 renames the messages.)
2. `EntityEvent.event` gains the case `RichText rich_text_changed = 3;`. It carries the
   saved document (ref, doc, updated_at).
3. Comment on `RichTextService.Get`:
   - A declared rich-text property with nothing saved returns an empty `doc` and
     epoch `updated_at`.
   - `NOT_FOUND` means the entity doesn't exist.
   - `INVALID_ARGUMENT` means the property isn't a declared rich-text property of the
     entity's structure.

Regenerate both TS stubs.

## Server changes
- `RichTextService` needs the `WatchHub`. Wire it in `main.rs` the way `EntityService`
  gets it, and update `test_support.rs`.
- **Validation (Get and Put):** load the entity's `structure_type`. A missing entity is
  `NotFound`. If `property_id` isn't in `structures::richtext_properties(type)`, return
  `InvalidArgument`.
- **Conflict check (Put):** in the same transaction, read the current `updated_at` (or
  "none") and compare it with `expected_updated_at` per the semantics above. On a
  mismatch, return `FailedPrecondition` and write nothing.
- **Strictly increasing `updated_at`:** set the doc's `updated_at` to
  `max(now_ms, previous_ms + 1)`. Two saves in the same millisecond must not look equal.
- **Entity touch:** after the doc write, set `entities.updated_at` to the same value in
  the same transaction.
- **Events (after commit):** publish `rich_text_changed` with the saved `RichText`, then
  `upserted` with the freshly loaded entity (`entity::load_entity`), because links,
  `referenced_dates` and `updated_at` changed. Use the same order every time.
- **Get:** replace `NotFound` for an unsaved declared doc with an empty `doc` and epoch
  `updated_at`.

## Tests (in `richtext.rs`)
At least:
- `put_with_stale_expected_updated_at_fails`
- `put_with_matching_expected_updated_at_succeeds`
- `put_expecting_nothing_saved_fails_when_a_doc_exists`
- `put_to_missing_entity_is_not_found`
- `put_to_undeclared_property_is_invalid`
- `put_publishes_rich_text_changed_and_upserted`: subscribe to the hub before the Put
  and assert both events
- `put_bumps_entity_updated_at`
- `get_unsaved_declared_doc_is_empty`

## Out of scope
- Client changes (T05, T06): current clients keep working, because they send no
  expectation and still handle `NotFound`.
- Watch snapshot and revisions (T14).
- Any `EntityService` change.

## Done when
- The proto has the new field, event case and comments; both TS stubs are regenerated
  and `calcifer` and `mcp-server` still build and typecheck.
- All the tests above pass, along with the existing suite.
- `docs/reference/data-model.md` describes the new Put/Get semantics and the event.

## Commits
1. `proto: add RichText conflict check and rich_text_changed event (I-17, I-18)`, with
   regenerated stubs.
2. `server: validate, conflict-check and publish RichText writes (I-17, I-18)`
3. `docs: document RichText conflict check and events (I-17, I-18)`
