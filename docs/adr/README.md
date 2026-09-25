# Architecture Decision Records

One file per decision: the context that forced it, what was decided, and what it costs
us. ADRs are immutable once accepted — if a decision changes, add a new record that
supersedes the old one rather than editing history.

| # | Decision | Status |
|---|---|---|
| [1](0001-polymorphic-entity-model.md) | One polymorphic `Entity` type, differentiated by Structure | Accepted |
| [2](0002-schema-first-protobuf.md) | Author the data model in protobuf, generate both languages | Accepted |
| [3](0003-server-authoritative-link-graph.md) | Derive the link graph server-side from content | Accepted |
| [4](0004-dates-without-a-dateref-entity.md) | Dates are a calendar surface, not an entity | Accepted (supersedes the `DateRef` sketch) |
| [5](0005-single-user.md) | Single-user, no auth, no sync | Accepted |
| [6](0006-drop-the-lexical-comparison.md) | Drop the TipTap-vs-Lexical comparison; standardise on TipTap | Accepted (supersedes the editor comparison plan) |
| [7](0007-server-owned-structure-registry.md) | The server owns the structure registry; clients fetch it | Accepted |
| [8](0008-server-builds-entities.md) | Clients send intent; the server builds entities | Accepted |
| [9](0009-watch-fed-replica.md) | The frontend is a full replica fed by Watch | Accepted |

Related: [`../reference/data-model.md`](../reference/data-model.md) for the shapes
themselves, [`../specs/`](../specs/) for how built features work, and
[`../research/`](../research/) for the external prior art behind some of these calls.

## Decisions not yet recorded

Real choices that predate this log and are currently only described in the reference
docs. Worth writing up if they come back into question:

- **Rust + tonic + sqlx + SQLite** over the originally sketched Go + Connect-RPC + sqlc.
- **A local, in-process embedding model** (`all-MiniLM-L6-v2`) over a hosted embeddings
  API — no API key, nothing leaves the machine.
- **`RichTextService` split from `EntityService`**, so list responses stay lean and
  autosave doesn't ship the whole entity per keystroke.
