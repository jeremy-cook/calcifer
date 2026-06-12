# Phase 8 — Part 0: Rename `title` → `name`

Part of `app-plan.md` Phase 8. Pre-Phase-8 cleanup. **No Rust here** — this is pure TypeScript / proto find-replace.

## Goal

Rename the polymorphic identifier field on `Entity` from `title` to `name`. `title` reads as "document heading" — fine for `Note` and `DailyNote`, awkward for `Tag` and any future `Person`/`Project`. `name` is the neutral identifier-shaped field; the entity page on `Note`/`DailyNote` styles it as a heading at render time.

## Why now (and not folded into a later part)

The proto schema is the source of truth, and Phase 8 (Part 3) hardens it into a SQLite column. Renaming **before** Phase 8 is a pure FE find-replace plus regen — cheap and isolated. Renaming **after** Phase 8 means a coordinated FE + BE + DB migration with backwards-compat shims. Doing it first means the new server's first migration locks in the cleaner name from day one.

## What changes

```diff
- message Entity { ... string title = 3; ... }
+ message Entity { ... string name = 3; ... }
```

Field number `3` is preserved — proto wire compatibility is irrelevant here since no server consumes the wire format yet, but field-number stability is good hygiene.

## Checklist

### Proto + codegen
- [ ] `proto/calcifer/v1/entities.proto`: `string title = 3;` → `string name = 3;` on `Entity`
- [ ] `pnpm proto:gen` (from `calcifer/`) regenerates TS into `gen/ts/`

### Source-of-truth structure metadata
- [ ] `calcifer/src/model/structures.ts`:
  - `title.editable` → `name.editable`
  - `title.derive` → `name.derive`
  - `STRUCTURES.DailyNote` keys updated accordingly

### Model layer
- [ ] `calcifer/src/model/store.ts`: every `entity.title` → `entity.name`; rename action helpers (e.g. `setTitle` → `setName`) and any selector returning a title
- [ ] `calcifer/src/model/linkSync.ts`: same
- [ ] `calcifer/src/model/backlinks.ts`: same
- [ ] `calcifer/src/model/richtext.ts`: same (likely just a type import)

### Routes
- [ ] `calcifer/src/routes/e.$id.tsx`: input/state for the page heading renames `title` → `name`; rename `isTitleEditable` → `isNameEditable`; placeholder text can stay user-facing as "Title" or change — UX decision below
- [ ] `calcifer/src/routes/s.$structureType.tsx`: list rows key off `entity.name`

### Other consumers
- [ ] Sidebar (search, +New, structure nav) — every `.title` reference
- [ ] Mention chip label fallback (when label isn't carried on the node, falls back to `entity.name`)
- [ ] Backlink rows
- [ ] Calendar references rows
- [ ] `createDailyNote(iso)` — sets `name` (not `title`) to `formatLongDate(iso)`
- [ ] `moveDailyNote(id, newIso)` — re-derives `name`

### Storage
- [ ] Bump localStorage key: `calcifer.entities.v1` → `calcifer.entities.v2` so any existing dev data is dropped cleanly. Acceptable since Phase 8 nukes localStorage anyway.

### Docs
- [ ] `app-plan.md`: update the `Entity` proto excerpt (§ Data Model) and any Verification line that says "title"

## UX decision: user-visible labels

The rename is internal to the data model. Visible UI text — the input placeholder on `Note`/`DailyNote` pages — can stay as "Title" since that's still what the user perceives there. On the structure list pages (Tags, future Person), the column heading naturally becomes "Name" instead of "Title".

Recommendation: leave user-facing strings alone in this part. They'll get adjusted naturally as new Structures land and reveal where "Title" actually reads wrong.

## Verification

1. `pnpm tsc --noEmit` clean
2. `pnpm dev`
3. Click through every page:
   - `/` (sidebar / home)
   - `/e/$id` for a Note — heading edits and persists
   - `/e/$id` for a Tag — heading shows name correctly
   - `/s/Note`, `/s/Tag`, `/s/DailyNote` — list rows show correct names
   - `/calendar` — DailyNote section header shows the day's long-format name
   - Mention picker (`@`) — chip labels render
   - Backlinks panel — source rows show correct names
4. `grep -rn "\.title" calcifer/src --include='*.ts' --include='*.tsx'` returns **only** unrelated hits (HTML `title` attributes, doc title, etc.) — no `entity.title` or `Entity['title']` remains.

## Done when

All 4 verification steps pass; no `Entity.title` references remain in the codebase.
