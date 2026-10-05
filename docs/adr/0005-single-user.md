# 5. Single-user, no auth, no sync

- **Status:** Accepted
- **Date:** 2026-03-31 (assumed from project start; `530f7b2`)

## Context

Calcifer is a personal knowledge base, built for and run by one person on one machine.
Nothing about the product requires more than that.

Most of the complexity in note-taking apps is downstream of multi-user: identity and
auth, per-user data scoping, conflict resolution, operational transform or CRDTs for
concurrent editing, presence, permissions, and an always-on hosted backend. Each is
substantial, and each is load-bearing only if someone else is using the app.

The realistic risk is the opposite of over-building: adopting a "just in case"
multi-user posture early makes every subsequent feature more expensive, in service of
a requirement that may never arrive.

## Decision

Assume exactly one user. Explicitly out of scope:

- Authentication and authorisation
- Multi-tenant data scoping
- Server-authoritative sync, OT, or CRDTs for concurrent editing
- Presence and sharing

The server binds `127.0.0.1` by default (UI and API on `:8080`, MCP on `:8081`; set
`CALCIFER_ADDR` to bind the UI and API elsewhere, `CALCIFER_MCP_ADDR` for MCP) with no auth layer (and no CORS: the UI is served
from the same origin), and SQLite
runs as a local file (`server/calcifer.db`). Live update across browser tabs is handled
by the `Watch` server-streaming RPC — a fan-out broadcast, not a sync protocol, because
there is only ever one writer's intent to reconcile.

Loopback alone doesn't keep out a web page open in a browser on the same machine, so
`server/src/local_only.rs` checks every request: both listeners answer 403 unless `Host`
names this machine (`localhost`, a loopback IP, or the listener's own address), which
defeats DNS rebinding; and MCP also answers 403 to a non-loopback `Origin` and 415 to a
POST that isn't `application/json`, which blocks cross-site requests that skip the CORS
preflight (a `no-cors` `text/plain` POST could otherwise run a tool).

## Consequences

- No auth code, no session handling, no permission checks anywhere in the stack.
- SQLite is sufficient, and stays sufficient. No connection pooling concerns beyond a
  handful of connections, no network database, no hosted infrastructure.
- Last-write-wins is an acceptable conflict policy; two tabs editing one document is a
  tolerable edge case rather than a correctness problem.
- **The server is unauthenticated.** It binds loopback by default; if bound to another interface
  on a shared or untrusted network, anyone routable can read and write the entire knowledge base. This is
  acceptable only under the local-machine assumption, and is the first thing that must
  change if the app is ever exposed.
- Multi-user later is not an increment. It would require revisiting storage, identity,
  the editing model, and the transport — effectively a different product.
- AI agent writes are indistinguishable from human writes in the data model, since
  there is no user identity to attribute them to. Attribution would need the deferred
  provenance work.
