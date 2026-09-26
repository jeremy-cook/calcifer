# T21 · FE write helpers; no proto or Connect imports in components

**Area:** fe · **Issues:** I-32 (D1)

## Problem
Components build proto messages directly (e.g. `routes/e.$id.tsx`,
`components/entity/EntityRelationsField.tsx` use `create(…Schema, …)` from
`@bufbuild/protobuf`) and branch on `ConnectError` codes (e.g.
`components/calendar/DailyNoteDateField.tsx`). ADR 2 wants components to *read* proto
types; writes should go through `model/`.

## Requirements
- In `calcifer/src/model/`, add typed write helpers on top of the existing
  `useSetProperty`, e.g. `setRelations(entity, propertyId, ids)`, `setDate(entity,
  propertyId, iso | null)`, `setSelect(entity, propertyId, key | null)`. Name and shape
  them to fit the call sites. Don't add helpers nobody calls.
- Add `isAlreadyExists(err)` next to `isNotFound` in `model/api.ts`. (Since T06,
  `isNotFound` has no callers. Use it where a moved check needs it, otherwise delete it.)
- Move every proto message construction and every `ConnectError` or `Code` check out of
  `components/`, `routes/` and `layouts/` into `model/`.
- Type-only imports of generated types (`import type { Entity } from
  '@calcifer/proto/…'`) are fine and expected. Generated enums (e.g. `PropertyKind`) are
  fine too.
- Follow `CLAUDE.md` component style for anything you touch.

## Out of scope
Behaviour changes. Model-layer duplicates (T22).

## Done when
- `grep -rln "@bufbuild/protobuf\|@connectrpc" calcifer/src/components calcifer/src/routes calcifer/src/layouts`
  finds nothing.
- `pnpm build` and `pnpm lint` pass.
- Browser checks to report: tag add/remove, date set/clear, select change, and the
  daily-note move conflict message all still work.
- `ISSUES.md`: move I-32 to Resolved.

## Commits
1. `fe: route property writes and error checks through model helpers (I-32)`
2. `docs: resolve I-32`
