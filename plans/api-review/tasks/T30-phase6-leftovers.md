# T30 · One structure-type read per relation target; ADR 2 versions; quiet `pnpm install`

**Area:** server, docs, fe, mcp · **Issues:** I-62, I-63, I-64

## Background
- **I-62:** since T26, `replace_scoped_links` (`server/src/link_store.rs`) reads each
  target's `structure_type` to record the real type and to drop a gone target. For a
  relation write, `check_relation_targets` has already read the same row to validate
  the ref, then throws the type away (it returns bare `MentionRef { id }`).
- **I-63:** `docs/adr/0002-schema-first-protobuf.md` (Consequences) says the two TS
  consumers "pin different `@bufbuild/protobuf` versions". Since T29 both pin
  `@bufbuild/protobuf` 2.15.0, `@bufbuild/protoc-gen-es` 2.15.0 and `@bufbuild/buf`
  1.68.2 exactly.
- **I-64:** pnpm 10 skips dependency build scripts unless allowed, so `pnpm install`
  prints "Ignored build scripts: @bufbuild/buf" in both packages. `buf` runs from its
  platform binary package, so the script isn't needed.

## Requirements
- **I-62:** `replace_scoped_links` takes targets that already carry their real
  `structure_type` and does no per-target lookup. `check_relation_targets` returns
  those targets from the read it already does. The rich-text path gets them from one
  helper that reads each mention's type and drops a gone target (same behaviour as
  today). No behaviour change: gone targets are still skipped, and links still record
  the target's real type.
- **I-63:** amend the ADR bullet: both consumers pin the same versions and generate
  with a local plugin; the remaining wart is two separate generated stubs. Date it.
- **I-64:** in each package's `pnpm-workspace.yaml`, list the dependencies whose build
  scripts aren't needed under `ignoredBuiltDependencies` (`@bufbuild/buf`, `esbuild`, and
  `msw` in `calcifer`; the latter two were ignored before T29 too), so the warning goes
  away without running the scripts.

## Tests
- `cd server && cargo fmt --check && cargo test` (existing tests cover gone targets,
  real types and relation validation).
- A clean `pnpm install` (no `node_modules`) in `calcifer` and `mcp-server` prints no
  ignored-build-script warning;
  `pnpm proto:gen` still works in both (and leaves the stubs unchanged).

## Out of scope
I-61 (deferred by the user, 2026-09-29).

## Done when
- A relation write reads each target's `entities` row once.
- The ADR matches both `package.json`s.
- `ISSUES.md`: I-62, I-63, I-64 moved to Resolved.
