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
- [ ] `cd server && cargo fmt --check` passes (whenever `server/` changed; from T24 on,
      I-41).
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
- **Before the first server boot after T12**, gate G2 must be cleared, because T12's
  migration deletes rows. The user waived the backup (D5).
- For browser checks, use the `run` skill or the Firefox devtools tools. The
  `mcp__calcifer__*` tools act as "the agent" for concurrency checks, e.g. append to a
  note while it's open in the browser. They talk to whichever server is on `:8080`, so
  point that server at the scratch DB first.

---

## Status (2026-09-28)

Every task (T01–T29) is done and reviewed. Phase 6 closed 2026-09-28. Checks on the
final head: `cargo fmt --check`, `cargo test` (98 passed), `calcifer` build, lint and
`proto:lint`, `mcp-server` typecheck, `pnpm test:md`, and `pnpm test:tools` (first run on
a fresh scratch server, after T28). Browser checks on a scratch DB: T27's single
`ResolveEntity` per mention create; T25's live tag drop across tabs; T26's search by
split word and mention label (through the MCP tools); after T29, load and edit on
`@bufbuild/protobuf` 2.15.0. The browser pass over T19–T23 (renamed RPCs, write helpers,
merged create/delete, daily-note moves) passed on 2026-09-28 (`93b5d9e`); T23's
`text`/`number`/`relation` editors aren't reachable, since no structure declares them.

**Follow-up (2026-09-29):** T30 fixed I-62, I-63 and I-64 (D13). Checks: `cargo fmt
--check`, `cargo test` (98 passed), `calcifer` `proto:gen`, build, lint and `proto:lint`,
`mcp-server` `proto:gen`, typecheck, `pnpm test:md`, and `pnpm test:tools` on a fresh
scratch server (the first run failed once with `database is locked` during embedding,
I-40; the rerun passed). I-61 is deferred. The user decides when to merge `api-review`
to `main`.

**Triage (2026-09-26):** every unassigned issue was checked against the code and still
holds. All of them are fixed in Phase 6 (D12); I-58 is new, found during triage. None
is deferred beyond D9's (I-36, I-40, I-42, I-45, I-49) and I-34.

- **Live DB:** the first start of a server built from this branch against
  `server/calcifer.db` runs T12's migration. It deletes the 27 rich-text pointer rows
  (24 Note, 2 Todo, 1 DailyNote; counted read-only 2026-09-26) and leaves the other 8
  `properties` rows. The user approved it without a backup (D5).
- **Live DB dead ref (I-14, D11):** Todo "Ship" (`14d2d854…`) has `tags` =
  [deleted Tag `b744dbdd…`, `urgent-review`]. No product code cleans it up. The user
  either recreates the DB or runs this one-off with the server stopped (the tech lead
  never runs it). Checked read-only: it matches one row and leaves only `urgent-review`:
  ```sql
  UPDATE properties SET value_blob = x'3A2D' || substr(value_blob, 48)
  WHERE entity_id = '14d2d854-f26b-4148-a332-ef03e5b824d9' AND property_id = 'tags'
    AND length(value_blob) = 92
    AND CAST(substr(value_blob, 7, 36) AS TEXT) = 'b744dbdd-87f2-4884-afba-6f87b3cffe3c';
  ```
- **Ports:** Finnegan (`~/Projects/Finnegan`) wasn't running for Phase 3's checks, and the
  ports were freed afterwards. Ask before stopping it for a later phase's checks.
- **MCP server:** since T17 a Calcifer MCP process started before `c6ca637` calls the
  removed `Retrieve`, so `search_notes` fails against this branch. The user restarts it
  (`/mcp`) before relying on the agent tools against a server built from this branch.
- Open questions for the user: none. D9 (defer rare concurrency) applies to triage.
- **Phase 6 checks for the tech lead:** after T25, delete a Tag used by a Todo in the
  browser (scratch DB) and see the Todo's tags drop it live. After T27, the mention-create
  checks in T27's *Done when*. After T28, `pnpm test:tools` on the first run against a
  fresh scratch server.

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
| D5 | May T12 delete the stored `richtext` property rows (data change)? | Yes, via migration, after backing up `calcifer.db`. I-14's dead-ref cleanup stays with I-14. | Yes: the migration deletes only the stored rich-text pointer rows (`content` on Note, DailyNote, Todo); nothing else in `properties`. **No backup** (user: the live DB holds no real content). The tech lead still shows the row count before booting against the live DB. 2026-09-26 |
| D6 | Unused property kinds `text`, `number`, `relation` (C3): remove or render? | Remove them, and reserve their enum values and field numbers. Re-add them when a structure needs one. | **Render them** (not the recommendation): keep `text`, `number`, `relation` and add FE rendering. T23 runs variant "render"; G4 doesn't apply. 2026-09-25 |
| D7 | I-13: adopt buf's RPC naming (wrapped responses) or configure lint exceptions? | Adopt it (T19 variant A). Every consumer is being touched anyway, and wrapped responses leave room to grow (e.g. `created` on Resolve). | Adopt buf naming, T19 variant A (as recommended). 2026-09-25 |
| D8 | Phase 1 check-in: fix I-37 and I-39 now or later? | Now, as a short task before T07 (both lose data) | Fix both in Phase 1, as T06a (server) and T06b (fe), run in parallel. The tech lead resolves both in `ISSUES.md` at integration. 2026-09-25 |
| D9 | Phase 1 close: fix I-40 and I-42 (rare races and failure paths)? | Park I-42 until after Phase 2 | **Defer both, and don't treat rare concurrency as work** (user, 2026-09-25): Calcifer is a single-user local app, so clashes are very unlikely. The tech lead logs such findings as deferred and doesn't add them to task scope. 2026-09-25 |
| D10 | I-14: when an entity is deleted, strip its id from other entities' relation values on the server, or have readers ignore dead refs? | Strip on delete in the same transaction and publish upserts; no FE change | Server strips on delete (as recommended). T25. 2026-09-26 |
| D11 | I-14: clean up the live DB's one existing dead ref (Todo "Ship" → deleted Tag)? | An idempotent sweep at server start | **No cleanup code** (not the recommendation): "just do a one-off script, or delete the database and we can create it again from new". The product gets no sweep or migration. The tech lead recorded a checked one-off `UPDATE` under Status for the user to run, if they don't recreate the DB. 2026-09-26 |
| D12 | Triage of the unassigned issues: verdicts and grouping | Fix all 15 (none defer or won't-fix; all small, none a race) in T24–T29; log I-58 (found in triage) and fix it with I-53; no reindex of existing docs for I-54 (each updates on its next save); add `cargo fmt --check` to the review checklist | Approved as proposed. 2026-09-26 |
| D13 | Phase 6 leftovers I-61 to I-64: fix or defer? | Fix I-62, I-63, I-64 in one small task (T30); I-61 needs an escape syntax first (proposed: backslash inside `[[…]]`) | Fix I-62 to I-64 in T30; **defer I-61** (user, 2026-09-29) |

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
| I-41 | `cargo fmt --check` fails on committed server code (found in T06a) | low | T24 |
| I-42 | A rich-text save that fails for a non-conflict reason is dropped silently (found in T06b) | medium | deferred (D9) |
| I-43 | Creating a mention or tag sends Resolve twice (found in T07 checks) | low | T27 |
| I-44 | `test:tools` semantic assertion fails on a cold embedding model (found in T07 checks) | low | T28 |
| I-45 | Two untitled Tags created at once can pick the same free name (found in T08) | low | deferred (D9) |
| I-46 | `Resolve` by name can create an undated DailyNote, getting around `creatable` (found in T09) | low | T24 |
| I-47 | The MCP server hard-codes the `content` rich-text property instead of reading the registry (found in T12) | low | T28 |
| I-48 | Entity ordering relies on the query plan (`load_entity`; List ties) (found in T13) | low | T24 |
| I-49 | Two writes to one entity can publish events out of commit order; the browser's replica has the same shape (found in T14, T15) | low | deferred (D9) |
| I-50 | `PutRichText` with an empty `entity_id` answers `NOT_FOUND` (found in T19) | low | T24 |
| I-51 | Content mention links trust the doc's `structureType`; null ones are dropped (found in T20) | low | T26 |
| I-52 | Date strings aren't format-checked (chips, `PropertyValue.date`, MCP parser) (found in T20) | low | T24 (server), T28 (mcp), T29 (proto comment) |
| I-53 | `get_note` renders stale mention labels and loses `[[name\|label]]` targets (found in T20) | low | T28 |
| I-54 | Search text drops chips and splits words across marks (found in T20) | low | T26 |
| I-55 | `proto:gen` depends on the remote buf plugin and fails under rate limits (found in T20) | low | T29 |
| I-56 | Create and resolve callbacks are never stable (found in T22) | low | T27 |
| I-57 | Relation pickers: any-structure `relations` renders nothing; self is offered (found in T23) | low | T27 |
| I-58 | `get_note` → write-back moves non-Note mentions onto a Note of that name (found in triage) | low | T28 |
| I-59 | Structure descriptions tell the agent to reference a Todo with `[[Name]]` (found in T28) | low | T26 |
| I-60 | `parse.ts` contains a literal NUL byte, so git shows it as binary (found in T28) | low | T29 |
| I-61 | MCP link syntax isn't escaped for names with `\|`, `]]` or a structure prefix (found in T28) | low | deferred (D13) |
| I-62 | Relation writes look up each target's structure type twice (found in T26) | low | T30 |
| I-63 | ADR 2 says the TS consumers pin different `@bufbuild/protobuf` versions (found in T29) | low | T30 |
| I-64 | pnpm ignores `@bufbuild/buf`'s build script (found in T29) | low | T30 |
| I-14 | Deleting an entity leaves dead refs in other entities' relation values (pre-existing) | medium | T25 (D10, D11) |

Existing issues touched along the way: **I-15** (closed by T08), **I-16**
(closed by T09), **I-13** (closed by T19). **I-14** was left out of Phases 1–5 and
is fixed in Phase 6 (T25).

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
| T08 | [Create by intent and Rename replace Create({ entity }) and Update](tasks/T08-create-rename.md) | proto, server, fe, mcp | T07 | none | done (c6ae5ff, 1a8b312, f9b9b0a, 7c5fa96, 6edc2cd) |
| T09 | [Server write validation (types, flags, lookup order, messages, kinds)](tasks/T09-write-validation.md) | server | T08 | none (regenerates stubs) | done (f79d385, 4daf8b2, 8c640b1, 524d4b9, d1781fc, 959c1aa, 83541fb, fcced6c, 15fa0b2, a63eb20) |
| T10 | *merged into T07 and T08 (D4)* | | | | n/a |
| T11 | *merged into T12 (D4)* | | | | n/a |
| T12 | [Address rich text by declared property; drop stored RichTextRefs](tasks/T12-richtext-by-property.md) | proto, server, fe, mcp, docs | T09, G2 | none | done (226d0c0, e405428, ffc5fa1, b3bc618, 883c039) |
| **Phase 3: sync** |||||
| T13 | [Batch-load entities (List/snapshot in constant queries)](tasks/T13-batch-load.md) | server | T12 | none | done (4cc92ec) |
| T14 | [Watch: snapshot, revisions, resync](tasks/T14-watch-snapshot.md) | proto, server | T13 | none | done (b1cdcda, ed9ee58, 1f9b5a4) |
| T15 | [FE: replica from Watch; apply events; drop refetches](tasks/T15-fe-sync.md) | fe | T14 | none | done (bfdf3e1, b0889dc, a2efb11, a1667d9) |
| **Phase 4: message shapes and naming** |||||
| T16 | [Properties as a map](tasks/T16-properties-map.md) | all | T15 | none | done (5f839c9, 4e00f38) |
| T17 | [One Search RPC with mode and match ranges](tasks/T17-search.md) | proto, server, mcp | T16 | none | done (c6ca637, 7e3ad21, ce9591f) |
| T18 | [ListBacklinks request/response and self-link rule](tasks/T18-backlinks.md) | proto, server, mcp | T17 | none | done (6b22f57, c8b858a, 3d8f8e8, 9feaedc) |
| T19 | [buf naming and lint](tasks/T19-buf-naming.md) | all | T18 | none | done (ebeee38, feeea88) |
| T20 | [Document the contract and TipTap doc schema](tasks/T20-contract-docs.md) | docs, proto comments | T19 | none (regenerates stubs) | done (2faa8a4, dcf0625, 95b3325) |
| **Phase 5: frontend cleanup** |||||
| T21 | [FE write helpers; no proto/Connect imports in components](tasks/T21-fe-write-helpers.md) | fe | T20 | none | done (037f920, 27857fd) |
| T22 | [FE model-layer duplicates](tasks/T22-fe-duplicates.md) | fe | T21 | none | done (5a14ac1, 42cfb3b) |
| T23 | [Unused property kinds (remove or render, per D6)](tasks/T23-unused-kinds.md) | proto, server, fe, mcp | T22, G4 | none | done (0e98e65, 09958dd, a2d2368) |
| **Phase 6: triage follow-ups (D12)** |||||
| T24 | [`cargo fmt`, then request guards (Resolve, ordering, empty ids, dates)](tasks/T24-server-fmt-and-guards.md) | server, docs | T23 | T27, T28 | done (7526517, 7a701f0, 09040bc, 8853d3b, 8b28427, 267b72e, 9b6c4f2) |
| T25 | [Deleting an entity strips it from relation values](tasks/T25-strip-dead-relation-refs.md) | server, docs | T24 | T27, T28 | done (4b71cab, 87d1cc4) |
| T26 | [Content links use the target's real type; search text includes chips](tasks/T26-content-derivation.md) | server, docs | T25 | T27, T28 | done (ce66900, 3557ccc, 949aac0, d92d85a) |
| T27 | [FE: one resolve per mention create, stable callbacks, relation pickers](tasks/T27-fe-editor-and-picker-fixes.md) | fe | T23 | T24, T25, T26, T28 | done (0abc497, d6aa592, 1561537) |
| T28 | [MCP: registry-driven doc property, faithful `get_note`, strict dates, test poll](tasks/T28-mcp-round-trip.md) | mcp | T23 | T24, T25, T26, T27 | done (a260bc7, 8d7ae88, af8be1b, f8863ea, dc5ccac) |
| T29 | [Local pinned `protoc-gen-es`; refresh stale proto comments](tasks/T29-local-protoc-gen-es.md) | fe, mcp, proto | T24–T28 | none (regenerates stubs) | done (d7e8a20, eae5e48, fcbf6d0, 3692303, a4bda15, 9535686) |
| T30 | [One type read per relation target; ADR 2 versions; quiet `pnpm install`](tasks/T30-phase6-leftovers.md) | server, docs, fe, mcp | T29 | none | done (ecb4bd7, 1c64b97, 7b05e7a, 5b56341) |

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
- Phase 6: T24–T26 share `entity.rs` and `links.rs` and run in order, with T24's
  `cargo fmt` first so later diffs stay clean. T27 (fe) and T28 (mcp) touch neither and
  can run alongside any of them, or each other; at most one server task at a time. T29
  regenerates stubs and edits proto comments that T24–T26 made stale, so it runs last
  and alone. D10 means T25 has no FE half, so it pairs with T27.

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
