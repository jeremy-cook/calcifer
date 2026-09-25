# API review remediation plan

Addresses the architecture review in [`review.md`](review.md): make the server own writes
(Entity becomes output-only), make Watch able to keep the frontend's full copy in sync,
and clean up message shapes and the frontend API layer.

This file is the **tech lead's brief**. Engineers never read it; they read
[`engineer-brief.md`](engineer-brief.md) plus their own task file in [`tasks/`](tasks/).

---

## Tech lead: role

The tech lead runs the plan and does not write product code. Responsibilities:

1. **Gatekeeping.** Get the user's answers at each gate (below) before starting the
   tasks behind it. Record answers in the decision log in this file.
2. **Dispatch.** Start every task in a **fresh engineer context** (see Dispatch). One
   engineer, one task. Never hand an engineer a second task, and never fork your own
   context into an engineer.
3. **Review and verify.** Check each finished task against its *Done when* and the review
   checklist, and run the verification yourself. Don't take the engineer's word for it.
4. **Integrate.** Keep `api-review` green after every task. Keep the status board and
   `ISSUES.md` accurate.
5. **Triage.** Anything an engineer reports as out of scope is logged by you, either as
   a new `ISSUES.md` entry or as a note on a later task. It is never fixed in the task
   that found it.
6. **Report.** At the end of each phase, give the user a short summary: what landed,
   what was verified and how, and what's next or blocked.

### Branch

All work goes on the branch `api-review`, created from `main` before T01. Engineers
commit there. Only the user decides when to merge to `main`. Don't push unless the user
asks.

### Dispatch

Use the Agent tool with `subagent_type: "general-purpose"` (never `"fork"`, which would
carry your context along). Use this prompt, changing only the task ID:

> You are an engineer on the Calcifer repo (`/Users/bebop/Github/calcifer`, branch
> `api-review`). Read `plans/api-review/engineer-brief.md`, then
> `plans/api-review/tasks/<TASK-FILE>.md`, and do exactly that task. Don't read other
> task files or the plan README. When done, reply with the report format from the
> engineer brief.

If a task names a gate decision the engineer needs, add one line to the prompt:
`Decision <Dn>: <the user's answer>`.

**Follow-ups:** if a review finds problems, send them back to the **same** engineer
with SendMessage, which keeps that engineer's context. List the specific failures.
After two rounds that still fail, stop: either re-scope the task (split it or change the
approach) or ask the user.

**Parallelism:** run tasks one at a time unless the dependency graph below marks a pair
safe to run in parallel. Tasks that touch `proto/` or regenerate stubs never run in
parallel with anything. Engineers stage files by explicit path, so parallel commits
don't pick up each other's work.

### Review checklist (every task)

- [ ] The diff touches only the files the task lists, or the engineer explained why.
- [ ] Every *Done when* item holds, and you checked it yourself.
- [ ] `cd server && cargo test` passes (whenever `server/` or `proto/` changed).
- [ ] `cd calcifer && pnpm build && pnpm lint` passes (whenever `calcifer/` or `proto/`
      changed).
- [ ] `cd mcp-server && pnpm typecheck` passes (whenever `mcp-server/` or `proto/`
      changed).
- [ ] FE code follows `CLAUDE.md` (named props interfaces, early returns, and so on).
- [ ] Commits follow the repo convention: `area: summary (I-NN)`, one per unit, with the
      Co-Authored-By trailer.
- [ ] Nothing touched `server/calcifer.db`, the user's live data (see Live data).
- [ ] New findings from the engineer's report are logged.

Remember the Node PATH pin before any pnpm command:
`export PATH="/Users/bebop/.nvm/versions/node/v24.12.0/bin:$PATH"`.

### Live data and integration checks

`server/calcifer.db` holds the user's real notes. The server listens on the fixed port
`:8080`, and the MCP client and Vite proxy are hardwired to that port. Rules:

- Engineers never start the server, never run `pnpm test:tools`, and never apply
  migrations to `server/calcifer.db`. `cargo test` uses in-memory databases and is always
  safe.
- The tech lead does the integration checks (MCP `test:tools`, browser checks) against a
  **scratch** database:
  `cd server && DATABASE_URL=sqlite://$SCRATCH/calcifer-it.db cargo run`. This needs
  `:8080` free. If the user's own server is running, ask the user before stopping it.
- **Before the first server boot after T12**, back up `server/calcifer.db`, because T12's
  migration deletes rows. Only boot against it once gate G2 is cleared.
- For browser checks, use the `run` skill or the Firefox devtools tools. The
  `mcp__calcifer__*` tools act as "the agent" for concurrency checks, e.g. append to a
  note while it's open in the browser. They talk to whichever server is on `:8080`, so
  point that server at the scratch DB first.

---

## Gates

| Gate | Before | Ask the user |
|---|---|---|
| **G1** | T02 (and everything after T01) | Decisions D1–D4, D6 and D7, all at once with AskUserQuestion |
| **G2** | T12 | D5, asked just in time: approve T12's migration deleting the stored rich-text property rows, and approve backing up then booting against the live DB. Show the user the row count from T12's report |
| **G3** | Closing Phase 1, 2 and 3 | Nothing to ask. The tech lead runs the browser checks listed in the phase, then reports |
| **G4** | T23 (variant "remove") | Nothing to ask. The tech lead confirms read-only that the live DB holds no `text`, `number` or `relation` values: `sqlite3 -readonly server/calcifer.db "SELECT count(*) FROM properties WHERE substr(value_blob,1,1) IN (x'0A', x'11', x'2A')"` (the prost tags for fields 1, 2 and 5) must be 0. If it isn't, stop and ask the user |

## Decision log

Recommendations come from the planning session. Replace *pending* with the user's
answer and the date.

| # | Question | Recommendation | Answer |
|---|---|---|---|
| D1 | How should the frontend stay in sync (B3)? | Keep the full copy. Watch sends a snapshot on every (re)connect, then revisioned events. On lag, send a fresh snapshot. No `since` resume; that's overkill for a single user. | *pending* |
| D2 | Should name lookup require unique names, or be deterministic (A7)? | Deterministic: the oldest `created_at` wins (id breaks ties), documented in the proto. Only Tag stays unique. | *pending* |
| D3 | What happens when a browser save loses to an agent write? | Reload the server's version into the editor and show an inline "changed elsewhere" note. Up to ~300 ms of typing can be lost. No merge. | *pending* |
| D4 | How should the proto change? | Source-breaking changes are fine (all consumers are in this repo). Go additive first, migrate consumers, then remove, so every task leaves the branch green. New messages use buf naming. Everything else is renamed in T17. | *pending* |
| D5 | May T12 delete the stored `richtext` property rows (data change)? | Yes, via migration, after backing up `calcifer.db`. I-14's dead-ref cleanup stays with I-14. | *pending* |
| D6 | Unused property kinds `text`, `number`, `relation` (C3): remove or render? | Remove them, and reserve their enum values and field numbers. Re-add them when a structure needs one. | *pending* |
| D7 | I-13: adopt buf's RPC naming (wrapped responses) or configure lint exceptions? | Adopt it (T19 variant A). Every consumer is being touched anyway, and wrapped responses leave room to grow (e.g. `created` on Resolve). | *pending* |

---

## Issue IDs

T01 logs these in `ISSUES.md`. Every task cites them, so the numbering is fixed here.

| Issue | Finding | Sev | Closed by |
|---|---|---|---|
| I-17 | B1: Put sends no event, has no conflict check, no validation | high | T04–T06 |
| I-18 | B2: Get returns NotFound for "not saved yet" | low | T04–T06 |
| I-19 | C4: daily notes sorted by name | medium | T03 |
| I-20 | A1: Entity is both write input and read output; Create saves client links | high | T07, T10, T12 |
| I-21 | A2: default entities built in four places | medium | T07, T10, T12 |
| I-22 | A3: RichTextRef stored as a property value | medium | T11, T12 |
| I-23 | A4: daily notes created and moved differently per client | medium | T07, T09, T10, T12 |
| I-24 | A7: name lookup not deterministic; unknown structure types accepted | medium | T08 |
| I-25 | A8: wrong message on a unique-name clash | low | T08 |
| I-26 | C3: unused property kinds; `creatable`/`name_editable` not enforced | low | T08 (flags), T23 (kinds) |
| I-27 | B3: Watch can't keep a full copy in sync; refetch storm | high | T13–T15 |
| I-28 | C1: `repeated Property` should be a map | medium | T16 |
| I-29 | A5: two search RPCs; bracket snippets | low | T17 |
| I-30 | A6: ListBacklinks request/response; self-link disagreement | low | T18 |
| I-31 | C2: contract and TipTap doc schema undocumented | low | T20 |
| I-32 | D1: proto/Connect details leak into components | low | T21 |
| I-33 | D2: duplicated model-layer code | low | T15, T22 |
| I-34 | D3: relation edits resend the whole list | low | deferred |
| I-35 | P2: pruning an empty daily note can delete agent content | medium | T06 |

Existing issues touched along the way: **I-15** (closed by T07/T10/T12), **I-16**
(closed by T08), **I-13** (closed by T19). **I-14** is out of scope; suggest it to the
user after Phase 2.

---

## Tasks and dependencies

| ID | Task | Area | Depends on | Parallel-safe with | Status |
|---|---|---|---|---|---|
| T01 | [Log review findings in ISSUES.md](tasks/T01-log-issues.md) | docs | none | none | todo |
| T02 | [ADR 8 (server-owned writes) and ADR 9 (Watch replica)](tasks/T02-adrs.md) | docs | T01, G1 | T03 | todo |
| **Phase 1: stop losing data** |||||
| T03 | [Sort daily notes by date](tasks/T03-daily-note-sort.md) | fe | T01 | T02 | todo |
| T04 | [RichText Put: validate, conflict-check, publish events; Get returns empty](tasks/T04-richtext-put-server.md) | proto, server | T01, G1 | none (touches proto) | todo |
| T05 | [MCP: conflict-safe appends](tasks/T05-mcp-conflict-safe-append.md) | mcp | T04 | T06 | todo |
| T06 | [FE: live rich text, conflict handling, safe prune](tasks/T06-fe-live-richtext.md) | fe | T04 | T05 | todo |
| **Phase 2: server owns writes** |||||
| T07 | [New write RPCs: Create by intent, Rename, Resolve (additive)](tasks/T07-write-rpcs.md) | proto, server | T02, T06 | none | todo |
| T08 | [Server write validation (types, flags, lookup order, messages, kinds)](tasks/T08-write-validation.md) | server | T07 | none (regenerates stubs) | todo |
| T09 | [MCP: move to Resolve](tasks/T09-mcp-resolve.md) | mcp | T07 | T10 | todo |
| T10 | [FE: move writes to Create/Rename/Resolve/SetProperty; delete builders](tasks/T10-fe-writes.md) | fe | T07 | T09 | todo |
| T11 | [FE: address rich text by registry, not by property value](tasks/T11-fe-richtext-addressing.md) | fe | T10 | none | todo |
| T12 | [Remove the old write surface and stored RichTextRefs](tasks/T12-remove-old-surface.md) | proto, server, docs | T08, T09, T11, G2 | none | todo |
| **Phase 3: sync** |||||
| T13 | [Batch-load entities (List/snapshot in constant queries)](tasks/T13-batch-load.md) | server | T12 | none | todo |
| T14 | [Watch: snapshot, revisions, resync](tasks/T14-watch-snapshot.md) | proto, server | T13 | none | todo |
| T15 | [FE: replica from Watch; apply events; drop refetches](tasks/T15-fe-sync.md) | fe | T14 | none | todo |
| **Phase 4: message shapes and naming** |||||
| T16 | [Properties as a map](tasks/T16-properties-map.md) | all | T15 | none | todo |
| T17 | [One Search RPC with mode and match ranges](tasks/T17-search.md) | proto, server, mcp | T16 | none | todo |
| T18 | [ListBacklinks request/response and self-link rule](tasks/T18-backlinks.md) | proto, server, mcp | T17 | none | todo |
| T19 | [buf naming and lint](tasks/T19-buf-naming.md) | all | T18 | none | todo |
| T20 | [Document the contract and TipTap doc schema](tasks/T20-contract-docs.md) | docs, proto comments | T19 | none (regenerates stubs) | todo |
| **Phase 5: frontend cleanup** |||||
| T21 | [FE write helpers; no proto/Connect imports in components](tasks/T21-fe-write-helpers.md) | fe | T20 | none | todo |
| T22 | [FE model-layer duplicates](tasks/T22-fe-duplicates.md) | fe | T21 | none | todo |
| T23 | [Unused property kinds (remove or render, per D6)](tasks/T23-unused-kinds.md) | proto, server, fe, mcp | T22, G4 | none | todo |

Status values: `todo` · `in progress (engineer: <agent name>)` · `review` · `done (<commit>)` · `blocked (<why>)`.

### Why this order
- Phase 1 is first because B1 loses user data today. T03 is a trivial visible bug and
  fits anywhere.
- Phase 2 is additive-first (T07/T08), then consumers move (T09–T11), then removal
  (T12). The branch compiles and works after every task.
- Phase 3 comes after Phase 2 so the snapshot and events carry the final entity shape,
  and so the refetch removal doesn't have to deal with the old builders.
- T16 and T19 are mechanical changes across every consumer. Doing them after the big
  rewrites means less code to churn. T17 and T18 go before the buf rename (T19) so it
  renames the final set of messages, not ones about to be deleted.

### Phase-end browser checks (G3)
- **Phase 1:** open a note in the browser; append to it with `mcp__calcifer__append_to_note`;
  the text appears in the editor without a reload, and a later browser edit keeps it.
  Daily notes list newest date first. Leaving an empty daily note's day still prunes it.
- **Phase 2:** "+ New" for each creatable structure opens an entity with a focused title;
  renaming while the agent sets a property keeps both; moving a daily note's date renames
  it; `[[wikilink]]` and `#tag` creation still work; an existing note from before T12
  still opens with its content.
- **Phase 3:** the network panel shows no `List` calls after startup; typing in a note
  sends no list refetches; a second tab sees edits live; killing and restarting the
  server makes the tab resync.
