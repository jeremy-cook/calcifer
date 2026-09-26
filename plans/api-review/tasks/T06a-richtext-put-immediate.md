# T06a · RichText Put: a racing save fails with FailedPrecondition, not Internal

**Area:** server · **Issues:** I-37

## Background
`RichText.Put` (`server/src/services/richtext.rs`, `put`) takes `expected_updated_at`.
Inside one transaction it reads the stored `updated_at`, then writes. A mismatch
returns `FAILED_PRECONDITION`. The browser editor and the MCP append both react to that
code: the editor reloads and shows a notice, and MCP retries.

## Problem
`put` opens its transaction with `self.pool.begin()`, which is a DEFERRED transaction.
Pools opened by `db::connect` run WAL with a `busy_timeout`. But once a deferred
transaction has read, it can't upgrade to a writer if another connection committed
after its read began. SQLite then returns `SQLITE_BUSY` straight away, and the
timeout doesn't help. So when two conditional `Put`s race, the loser gets `Internal`
instead of `FailedPrecondition`. See I-37 in `ISSUES.md`.

## Requirements
- Start `put`'s transaction with `BEGIN IMMEDIATE`, so it takes the write lock before
  its read and a racing `Put` waits on `busy_timeout` rather than failing. Check what
  the installed sqlx (0.9) offers, e.g. `Pool::begin_with("BEGIN IMMEDIATE")`, and use
  that. Don't hand-roll `BEGIN` and `COMMIT` as raw statements unless sqlx has no
  supported way.
- Change only `put`'s transaction. If other read-then-write transactions in
  `services/` have the same pattern, list them in your report. Don't change them.
- Add a comment at the `begin` call explaining why the transaction is IMMEDIATE.

## Test
`memory_pool()` (in `test_support.rs`) allows only one connection, so it can't show
this race. Add a test that:
- opens a **file-backed** pool through `db::connect` on a unique path in
  `std::env::temp_dir()`, and removes the file (and its `-wal` and `-shm`) at the end;
- seeds an entity with a declared rich-text property and one saved doc (reuse the
  existing helpers where you can);
- runs two `Put`s concurrently (e.g. `tokio::join!` on a multi-thread runtime, i.e.
  `#[tokio::test(flavor = "multi_thread")]`), both expecting the same stored
  `updated_at`;
- asserts that exactly one succeeds and the other fails with `FailedPrecondition`.

Name it for the behaviour, e.g. `racing_conditional_puts_give_one_success_and_one_conflict`.
Check that it **fails before your fix**, at least a few runs out of several; say how
often in your report. Don't add a dev-dependency if `std` does the job.

## Out of scope
Other services' transactions. Frontend and MCP changes.

## Done when
- The new test passes, and failed without the fix.
- `cd server && cargo test` passes.
- Don't edit `ISSUES.md`; another task runs in parallel, and the tech lead resolves
  the issue at integration.

## Commits
`server: take the write lock before RichText.Put reads (I-37)`
