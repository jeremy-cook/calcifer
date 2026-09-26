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
  build with `cd server && cargo build` (the `sqlx` macros check the schema of
  `server/.env`'s DB at compile time), then run the binary:
  `DATABASE_URL=sqlite://$SCRATCH/calcifer-it.db ./target/debug/calcifer-server`. The
  server creates and migrates the file itself. Don't use `cargo run` with the scratch
  URL (it compiles against the empty DB and fails) or the `sqlx` CLI (it can't load
  `vec0`). This needs `:8080` free. If the user's own server (or another project) holds
  it, ask the user before stopping it.
- **Before the first server boot after T12**, back up `server/calcifer.db`, because T12's
  migration deletes rows. Only boot against it once gate G2 is cleared.
- For browser checks, use the `run` skill or the Firefox devtools tools. The
  `mcp__calcifer__*` tools act as "the agent" for concurrency checks, e.g. append to a
  note while it's open in the browser. They talk to whichever server is on `:8080`, so
  point that server at the scratch DB first.

---

## Paused (2026-09-25), resumed 2026-09-26

Work paused at the user's request after T07 and resumed with T08 on 2026-09-26. The branch is green: `cargo test`,
`calcifer` build + lint and `mcp-server` typecheck pass, and Phase 1 plus T07 were
checked in the browser against a scratch DB.

To resume:
- Dispatch **T08** fresh (a first attempt was stopped before it committed anything;
  its partial proto edits were discarded).
- Finnegan (`~/Projects/Finnegan`) was stopped to free `:8080` and `:5173` for the
  checks. The user restarts it; ask again before stopping it for Phase 2's checks.
- Before relying on the agent tools against a server built from this branch, restart the
  Calcifer MCP server. A process started before T07 calls RPCs that no longer exist.
- Open questions for the user: none. D9 (defer rare concurrency) applies to triage.

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
| D1 | How should the frontend stay in sync (B3)? | Keep the full copy. Watch sends a snapshot on every (re)connect, then revisioned events. On lag, send a fresh snapshot. No `since` resume; that's overkill for a single user. | Keep the full copy; snapshot on every (re)connect, revisioned events, fresh snapshot on lag (as recommended). 2026-09-25 |
| D2 | Should name lookup require unique names, or be deterministic (A7)? | Deterministic: the oldest `created_at` wins (id breaks ties), documented in the proto. Only Tag stays unique. | Deterministic: oldest `created_at` wins, id breaks ties (as recommended). 2026-09-25 |
| D3 | What happens when a browser save loses to an agent write? | Reload the server's version into the editor and show an inline "changed elsewhere" note. Up to ~300 ms of typing can be lost. No merge. | Reload the server version with an inline "changed elsewhere" note; no merge (as recommended). 2026-09-25 |
| D4 | How should the proto change? | Source-breaking changes are fine (all consumers are in this repo). Go additive first, migrate consumers, then remove, so every task leaves the branch green. New messages use buf naming. Everything else is renamed in T17. | **Break in place** (not the recommendation): change the proto and every consumer together in each task, instead of additive → migrate → remove. Plan re-scoped to match, confirmed with the user 2026-09-25: Phase 2 is now T07 (Resolve), T08 (Create/Rename), T09 (validation), T12 (rich text by property); old T09–T11 merged in. T14/T15 stay separate: Watch's proto change is additive by nature (a new oneof case and field), so there's nothing to break in place. 2026-09-25 |
| D5 | May T12 delete the stored `richtext` property rows (data change)? | Yes, via migration, after backing up `calcifer.db`. I-14's dead-ref cleanup stays with I-14. | *pending* |
| D6 | Unused property kinds `text`, `number`, `relation` (C3): remove or render? | Remove them, and reserve their enum values and field numbers. Re-add them when a structure needs one. | **Render them** (not the recommendation): keep `text`, `number`, `relation` and add FE rendering. T23 runs variant "render"; G4 doesn't apply. 2026-09-25 |
| D7 | I-13: adopt buf's RPC naming (wrapped responses) or configure lint exceptions? | Adopt it (T19 variant A). Every consumer is being touched anyway, and wrapped responses leave room to grow (e.g. `created` on Resolve). | Adopt buf naming, T19 variant A (as recommended). 2026-09-25 |
| D8 | Phase 1 check-in: fix I-37 and I-39 now or later? | Now, as a short task before T07 (both lose data) | Fix both in Phase 1, as T06a (server) and T06b (fe), run in parallel. The tech lead resolves both in `ISSUES.md` at integration. 2026-09-25 |
| D9 | Phase 1 close: fix I-40 and I-42 (rare races and failure paths)? | Park I-42 until after Phase 2 | **Defer both, and don't treat rare concurrency as work** (user, 2026-09-25): Calcifer is a single-user local app, so clashes are very unlikely. The tech lead logs such findings as deferred and doesn't add them to task scope. 2026-09-25 |

---

## Issue IDs

T01 logs these in `ISSUES.md`. Every task cites them, so the numbering is fixed here.

| Issue | Finding | Sev | Closed by |
|---|---|---|---|
| I-17 | B1: Put sends no event, has no conflict check, no validation | high | T04–T06 |
| I-18 | B2: Get returns NotFound for "not saved yet" | low | T04–T06 |
| I-19 | C4: daily notes sorted by name | medium | T03 |
| I-20 | A1: Entity is both write input and read output; Create saves client links | high | T08 |
| I-21 | A2: default entities built in four places | medium | T07, T08 |
| I-22 | A3: RichTextRef stored as a property value | medium | T12 |
| I-23 | A4: daily notes created and moved differently per client | medium | T07 |
| I-24 | A7: name lookup not deterministic; unknown structure types accepted | medium | T09 |
| I-25 | A8: wrong message on a unique-name clash | low | T09 |
| I-26 | C3: unused property kinds; `creatable`/`name_editable` not enforced | low | T09 (flags), T23 (kinds) |
| I-27 | B3: Watch can't keep a full copy in sync; refetch storm | high | T13–T15 |
| I-28 | C1: `repeated Property` should be a map | medium | T16 |
| I-29 | A5: two search RPCs; bracket snippets | low | T17 |
| I-30 | A6: ListBacklinks request/response; self-link disagreement | low | T18 |
| I-31 | C2: contract and TipTap doc schema undocumented | low | T20 |
| I-32 | D1: proto/Connect details leak into components | low | T21 |
| I-33 | D2: duplicated model-layer code | low | T15, T22 |
| I-34 | D3: relation edits resend the whole list | low | deferred |
| I-35 | P2: pruning an empty daily note can delete agent content | medium | T06 |
| I-36 | ResolveByName returns a generic error when it loses a create race (found in T01) | low | deferred (D9) |
| I-37 | A racing RichText.Put fails with Internal, not FailedPrecondition (found in T04) | medium | T06a |
| I-38 | Stale comment in watch.rs (found in T04; richtext.ts half fixed in T06) | low | T14 (note) |
| I-39 | Leaving a new daily note within the save debounce deletes what was typed (found in T06) | medium | T06b |
| I-40 | Entity write transactions that read first fail with Internal under a race (found in T06a) | medium | deferred (D9) |
| I-41 | `cargo fmt --check` fails on committed server code (found in T06a) | low | *unassigned* |
| I-42 | A rich-text save that fails for a non-conflict reason is dropped silently (found in T06b) | medium | deferred (D9) |
| I-43 | Creating a mention or tag sends Resolve twice (found in T07 checks) | low | *unassigned* |
| I-44 | `test:tools` semantic assertion fails on a cold embedding model (found in T07 checks) | low | *unassigned* |

Existing issues touched along the way: **I-15** (closed by T08), **I-16**
(closed by T09), **I-13** (closed by T19). **I-14** is out of scope; suggest it to the
user after Phase 2.

---

## Tasks and dependencies

| ID | Task | Area | Depends on | Parallel-safe with | Status |
|---|---|---|---|---|---|
| T01 | [Log review findings in ISSUES.md](tasks/T01-log-issues.md) | docs | none | none | done (43e25e0) |
| T02 | [ADR 8 (server-owned writes) and ADR 9 (Watch replica)](tasks/T02-adrs.md) | docs | T01, G1 | T03 | done (7f5a8fa, 0b070c1) |
| **Phase 1: stop losing data** |||||
| T03 | [Sort daily notes by date](tasks/T03-daily-note-sort.md) | fe | T01 | T02 | done (0b12cf4, 1678503) |
| T04 | [RichText Put: validate, conflict-check, publish events; Get returns empty](tasks/T04-richtext-put-server.md) | proto, server | T01, G1 | none (touches proto) | done (4dea7a7, d68bb41, 81f1605) |
| T05 | [MCP: conflict-safe appends](tasks/T05-mcp-conflict-safe-append.md) | mcp | T04 | T06 | done (b5ec83e) |
| T06 | [FE: live rich text, conflict handling, safe prune](tasks/T06-fe-live-richtext.md) | fe | T04 | T05 | done (17b38cc, 20dc58d, 62775de, 971e98a) |
| T06a | [RichText Put takes the write lock first (racing save → FailedPrecondition)](tasks/T06a-richtext-put-immediate.md) | server | T06 | T06b | done (a3ba247) |
| T06b | [FE: flush pending saves before pruning a daily note](tasks/T06b-flush-before-prune.md) | fe | T06 | T06a | done (75815ae) |
| **Phase 2: server owns writes** |||||
| T07 | [Resolve replaces ResolveByName and CreateDailyNote; server-owned daily-note names](tasks/T07-resolve.md) | proto, server, fe, mcp | T02, T06a, T06b | none | done (ffbda48, aa5d882, 3644f2c, b4c3b09, 2391505, c2d4c67) |
| T08 | [Create by intent and Rename replace Create({ entity }) and Update](tasks/T08-create-rename.md) | proto, server, fe, mcp | T07 | none | in progress (engineer: T08 engineer) |
| T09 | [Server write validation (types, flags, lookup order, messages, kinds)](tasks/T09-write-validation.md) | server | T08 | none (regenerates stubs) | todo |
| T10 | *merged into T07 and T08 (D4)* | | | | n/a |
| T11 | *merged into T12 (D4)* | | | | n/a |
| T12 | [Address rich text by declared property; drop stored RichTextRefs](tasks/T12-richtext-by-property.md) | proto, server, fe, mcp, docs | T09, G2 | none | todo |
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
- Phase 2 breaks in place (D4): each task changes the proto and every consumer
  together and deletes what it replaces. Resolve (T07) goes first because it takes the
  daily-note paths off `Create({ entity })` and `Update`, so T08 can then remove both.
  Validation (T09) needs the final write surface. T12 goes last in the phase because
  it carries the G2 data migration. The branch compiles and works after every task.
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
  it; `@structure/Name` mentions and `#tag` creation still work in the browser, and `[[wikilink]]` through MCP; an existing note from before T12
  still opens with its content.
- **Phase 3:** the network panel shows no `List` calls after startup; typing in a note
  sends no list refetches; a second tab sees edits live; killing and restarting the
  server makes the tab resync.
