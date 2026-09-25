# T14 · Watch: snapshot, revisions, resync

**Area:** proto, server · **Issues:** I-27 (B3)

## Background
Read ADR 9 in `docs/adr/`: the frontend is a full replica fed by Watch. Your prompt
includes decision **D1**. Recommended: a snapshot on every (re)connect, then revisioned
events, and a fresh snapshot on lag, with no `since` resume.

Today (`server/src/watch.rs`, `EntityService::watch` in `services/entity.rs`):
- `WatchHub` is a `tokio::sync::broadcast` of `EntityEvent` with capacity 256.
- `watch` subscribes and **silently drops** `Lagged` errors.
- There's no snapshot, so clients call `List` separately and race with the subscription.
- Events: `upserted`, `deleted_id` and `rich_text_changed`, published by `EntityService`
  and `RichTextService`.
- `load_entities(pool, None)` (T13) loads everything in constant queries.

## Proto changes (additive)
```proto
message EntitySnapshot { repeated Entity entities = 1; }
message EntityEvent {
  oneof event {
    Entity upserted = 1;
    string deleted_id = 2;
    RichText rich_text_changed = 3;
    EntitySnapshot snapshot = 4;   // replaces the client's whole entity set
  }
  uint64 revision = 15;            // monotonic per server process
}
```
Comment the contract on `rpc Watch`:
- The first message on every stream is a `snapshot`.
- After it come events with increasing `revision`.
- Whenever the server can't guarantee continuity (the subscriber lagged), it sends a
  fresh `snapshot`, and the client must replace its whole set.
- A snapshot doesn't include rich-text docs; clients refetch any doc they hold.
- Revisions restart when the server restarts. Clients never compare revisions across
  streams.

Also comment `List`, `ListBacklinks` and `Search` as intended for the agent: the frontend
replica comes from Watch.

Regenerate both TS stubs. The current FE ignores the unknown `snapshot` case, so it keeps
working.

## Server changes
- `WatchHub::publish` assigns the revision.
  - Hold a `Mutex` across increment-and-send, so channel order equals revision order.
  - Add `current_revision()`.
  - Add `with_capacity(n)` for tests.
- `watch` builds the stream (e.g. `async-stream` or `futures::stream::unfold`; add the
  crate only if needed):
  1. `subscribe()` first.
  2. Read `R = current_revision()`.
  3. `load_entities(None)` and emit `snapshot` with `revision = R`.
  4. Forward received events, skipping those with `revision <= R`.
  5. On `Lagged`, repeat steps 2–4 with the same receiver (emit a new snapshot), then
     continue.
- Put a comment by the ordering explaining why nothing is lost:
  - an event with revision ≤ R was published after its transaction committed, so it's in
    the snapshot;
  - a later event may duplicate snapshot content, which is harmless because upsert and
    delete are idempotent.
- The first `snapshot` must arrive promptly, even with no writes happening.

## Tests
- `watch_starts_with_a_snapshot_of_existing_entities`
- `watch_events_after_snapshot_have_increasing_revisions`
- `watch_does_not_repeat_events_covered_by_the_snapshot`: publish before subscribing,
  then check.
- `watch_resyncs_with_a_snapshot_after_lag`: use `with_capacity(2)` and publish more than
  that without reading.

## Out of scope
FE changes (T15). A `since` parameter (not needed under D1).

## Done when
- The tests above pass, along with the suite.
- `calcifer` builds and `mcp-server` typechecks unchanged.
- `docs/reference/data-model.md` describes the Watch contract.

## Commits
1. `proto: give Watch a snapshot and revisions (I-27)`, with regenerated stubs.
2. `server: open Watch with a snapshot and resync on lag (I-27)`
3. `docs: document the Watch contract (I-27)`
