# Engineer brief

You've been given **one task file** in `plans/api-review/tasks/`. This brief is the
shared background; the task file is your scope. Do that task and nothing else.

## Scope discipline
- Change only what your task asks for. If you find a bug, a smell or a cleanup outside
  your scope, **don't fix it**; put it in your report. The tech lead logs it.
- Don't read other task files or `plans/api-review/README.md`. You may read
  `plans/api-review/review.md` for the finding your task cites.
- If your task can't be done as written (the code differs from what it describes, or a
  requirement conflicts with another), stop and report rather than improvising a
  different design.
- If the task says a decision was made (D1–D7), the tech lead gives you the answer in
  your prompt. If it's missing, ask in your report instead of guessing.

## The repo in one screen
- `proto/calcifer/v1/*.proto`: the schema. After editing, regenerate **both** TS
  consumers: `cd calcifer && pnpm proto:gen` and `cd mcp-server && pnpm proto:gen`.
  Rust regenerates on `cargo build` (tonic-build). A field named `ref` is `r#ref` in Rust.
- `server/`: Rust, tonic gRPC and SQLite via sqlx. Services are in
  `server/src/services/*.rs`, registered in `main.rs`. `link_store.rs` and `links.rs`
  derive links from TipTap JSON. `structures.rs` is the structure registry (ADR 7).
  `watch.rs` is the event hub. `test_support.rs` holds test helpers (`memory_pool`,
  `entity_service`, `note`, `tag`, `todo`).
- `calcifer/`: React, Vite, TanStack Query and Router, TipTap. The model layer is
  `src/model/*.ts`; components can't talk to clients directly. Component style rules are
  in the root `CLAUDE.md`. Follow them.
- `mcp-server/`: Node MCP server. Operations are in `src/tools.ts`, the MCP wrapper in
  `src/server.ts`. It has its own generated stubs (`src/gen/`, gitignored).
- `docs/adr/` (why), `docs/reference/data-model.md` (shapes and RPC surface; keep it
  accurate when you change the proto), `ISSUES.md` (known defects).

## Commands
Always pin Node first; fresh shells sometimes pick Node 16 and pnpm aborts:

```bash
export PATH="/Users/bebop/.nvm/versions/node/v24.12.0/bin:$PATH"
```

| Changed | Run before reporting |
|---|---|
| `proto/` | regenerate both TS stubs, then every row below |
| `server/` | `cd server && cargo test` |
| `calcifer/` | `cd calcifer && pnpm build && pnpm lint` |
| `mcp-server/` | `cd mcp-server && pnpm typecheck` |

## Live data: hard rules
`server/calcifer.db` is the user's real notes.
- **Never** start the server (`cargo run`), run `pnpm test:tools` or `pnpm smoke`, or
  open, migrate or delete `server/calcifer.db`. The tech lead runs integration checks
  against a scratch database.
- `cargo test` uses in-memory databases and is always safe.
- `sqlx::query!` checks SQL at compile time against `DATABASE_URL` from `server/.env`,
  which points at the live DB. **If your task adds a migration**, build and test against
  a scratch copy instead:
  ```bash
  cd server
  export DATABASE_URL=sqlite://$TMPDIR/calcifer-build.db
  sqlx database create && sqlx migrate run && cargo test
  ```
  The `sqlx` CLI can't load `vec0`, so `sqlx migrate run` stops with an error at the
  `vec` migration. The migrations before it are enough for the compile-time checks.
  Test the new migration itself in `cargo test`, whose in-memory pools load `vec0`.

## Tests
- Server behaviour changes need a Rust test in the module's `#[cfg(test)] mod tests`,
  named for the behaviour (e.g. `put_with_stale_expected_updated_at_fails`). Follow the
  existing tests in `services/entity.rs`.
- The frontend has no unit test runner. Verify with `pnpm build` and `pnpm lint`, and
  describe in your report what should be checked in the browser. The tech lead does
  browser checks.
- The MCP server: `pnpm typecheck`. You may update `src/tools-test.ts` to match new
  behaviour, but don't run it.

## Commits
- Work on the current branch (`api-review`). Never switch branches, rebase, push, or
  amend commits you didn't make in this task.
- One commit per unit, in this order when a task spans areas: `proto:` → `server:` →
  `fe:` → `mcp:` → `docs:`. Message format: `area: imperative summary (I-NN)`, matching
  `git log`. Regenerated stubs go in the commit that needs them.
- Stage files **by path** (`git add path/a path/b`), never `git add -A` or `.`; another
  engineer may be working at the same time.
- End every commit message with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  ```
- Update `ISSUES.md` or docs only if your task says to.

## Report format (your final reply)
```
Task: TNN, done | partial | blocked
Commits: <sha> <subject>, one per line
Done-when: each item from the task, with how you checked it
Checks run: the commands and their result (pass/fail, test counts)
Browser checks for the tech lead: what to click and what to expect (FE tasks)
Out-of-scope findings: file:line and one sentence each (or "none")
Open questions: (or "none")
```
