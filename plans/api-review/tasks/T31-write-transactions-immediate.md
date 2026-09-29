# T31 · Every write transaction takes the write lock first

**Area:** server · **Issues:** I-40

## Background
- **I-40:** `rename_entity`, `set_entity_property` and `delete_entity`
  (`server/src/services/entity.rs`) and the embed worker's chunk replace
  (`server/src/embed/worker.rs`) open DEFERRED transactions, read, then write. If another
  connection commits in between, SQLite refuses the upgrade to a writer with
  `SQLITE_BUSY` at once, ignoring `busy_timeout`: the RPC fails as `Internal`, or the
  worker drops a batch. `PutRichText` already opens with `BEGIN IMMEDIATE` (I-37).
- Deferred under D9 as unlikely, then observed on 2026-09-29: the first `test:tools` run
  on a fresh scratch server failed with `database is locked` while the embed worker
  dropped two batches. The competing writer is the app's own embed worker, not a second
  user (D14).

## Requirements
- One helper, `db::begin_write`, opens a write transaction with `BEGIN IMMEDIATE`, and
  every write transaction in the server uses it (including `PutRichText` and
  `persist_new_entity`, so the rule is simply "writes use `begin_write`").
- The file-backed scratch pool used by `PutRichText`'s race test moves to
  `test_support` (`ScratchDb`), so other race tests can share it.

## Tests
- A race test: two `set_entity_property` writes to one entity, run together on a
  file-backed pool for a few rounds; both succeed and both values are stored. It fails
  with `Internal: database is locked` before the fix.
- `cd server && cargo fmt --check && cargo test`.
- `pnpm test:tools` on a fresh scratch server passes on the first run, with no embed
  worker warnings in the server log.

## Out of scope
I-36, I-42, I-45, I-49 (still deferred under D9).

## Done when
- `grep -rn "\.begin()" server/src` finds no write transaction.
- The race test passes repeatedly.
- `ISSUES.md`: I-40 moved to Resolved.
