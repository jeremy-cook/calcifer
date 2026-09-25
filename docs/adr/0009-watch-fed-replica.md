# 9. The frontend is a full replica fed by Watch

- **Status:** Accepted
- **Date:** 2026-09-25 (context for I-27; implemented by later tasks)

## Context

In practice the frontend already holds a full copy of every entity: it lists them all
once and derives backlinks, the calendar and the todo views itself. For one user on one
machine ([ADR 5](0005-single-user.md)) that is reasonable. But the copy is built from
`List` plus cache invalidations, and `Watch` was never designed to keep it in sync:

- **Startup race.** The initial `List` and the `Watch` subscription start at the same
  time, so events that land between them are lost.
- **Silent loss.** The server drops events for a subscriber that falls behind, and
  nothing tells the client to reload.
- **Reconnects.** After a reconnect the client doesn't refetch, so anything missed while
  disconnected stays missing.
- **Refetch storm.** Every event and every mutation invalidates the entity list, and the
  server serves each reload as 1 + 4N queries. Each debounced keystroke save triggers a
  full reload; each `SetProperty` triggers two.
- **Rich text is invisible.** `RichText.Put` publishes no event and has no conflict
  check. When the agent appends to a note that's open in the browser, the editor never
  sees it, and its next debounced save overwrites the append.

Two ways to fix it were considered: drop the full copy and query per view, or make the
copy correct. Per-view queries would move backlinks, the calendar and todo filtering
back to the server and add a round trip to each view, to solve a data-size problem
Calcifer doesn't have.

## Decision

**Keep the full copy, and make `Watch` the only thing that feeds it.**

- **Watch opens with a snapshot.** Every connection, first or reconnect, starts with
  every entity, then streams events. Each event carries a revision number, increasing
  within the stream, so the client applies them in order on top of the snapshot.
- **No resume.** A client never asks for "events since revision N". A reconnect gets a
  fresh snapshot, which keeps the server free of an event log.
- **Lag means resync, never silent loss.** A subscriber that falls behind is sent a fresh
  snapshot in place of the events it missed.
- **The frontend applies events to its cache and never refetches the list.** Mutations
  don't invalidate it either; the Watch echo is how a write reaches the cache. The model
  layer owns this in one sync module; components don't see it.
- **Rich-text changes are events too.** A `Put` publishes an entity update (its links,
  `referenced_dates` and `updated_at` change) and a rich-text-changed event, so an open
  editor can take in an outside change.
- **`Put` is conflict-checked.** It takes the `updated_at` the writer last saw and fails
  with `FailedPrecondition` if the document has changed since. The MCP append, which
  reads, edits and writes back, gets the same protection.
- **When a browser save loses, the server's version wins.** The editor reloads the
  server's document and shows an inline "changed elsewhere" note. Typing since the last
  successful save, up to about one debounce interval (~300 ms), can be lost. There is no
  merge.
- **Filtered `List`, `ListBacklinks` and `Search` exist for the agent.** The frontend
  derives the same answers from its replica and doesn't call them.

### Changes to earlier ADRs

[ADR 5](0005-single-user.md) describes `Watch` as a fan-out broadcast, not a sync
protocol, and accepts last-write-wins. `Watch` is now a one-way replication stream from
the server to each client: snapshot, then ordered events. It is still not a sync
protocol in ADR 5's sense: there is no OT, no CRDT and no merge, and the server stays the
only place writes are reconciled. Last-write-wins still holds for entity writes. For
rich text, a stale save now loses to the version already on the server instead of
overwriting it.

## Consequences

- The startup race, silent loss, missed-while-disconnected events and the refetch storm
  all go away. The server does one full read per connection instead of one per event.
- The server sends the whole entity set in one response on every connection and every
  resync. That is fine at single-user sizes and is the first thing to revisit if the data
  gets large.
- A slow client costs a snapshot rather than a gap, so a burst of agent writes can make a
  lagging tab re-download everything.
- The frontend model layer owns a sync module that applies snapshots and events.
  Components read from the cache and never learn about the stream.
- Losing a race to the agent throws away up to ~300 ms of the author's typing. That is
  accepted as better than silently overwriting the agent's write, and much simpler than
  merging.
- The agent-facing RPCs (`List` filters, `ListBacklinks`, `Search`) have no frontend
  caller. Only the MCP server and their tests exercise them, so a regression there won't
  show up in the browser.
- This design assumes one user and a data set that fits in memory on every client. It
  would need revisiting if Calcifer became multi-user or the data got large.
