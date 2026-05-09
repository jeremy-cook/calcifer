# Phase 6 Part 6 — Slash date shortcuts

> Leadership context: see [`../app-plan.md`](../app-plan.md) Phase 6.
> Predecessor: [`phase-6-part-5-calendar-markers.md`](./phase-6-part-5-calendar-markers.md).

## Context

Phase 6 Part 4 deferred slash-keyword shortcuts (`/today`, `/tomorrow`, `/yesterday`) for date chips, with free-form NL parsing explicitly off the roadmap. The slash menu already has a "Today" item; adding two siblings closes the smaller half of that deferred bullet.

## Design

In `calcifer/src/lib/tiptap-extension-slash-command/items.ts`, under the `Inline` group:

- Replace `Today`'s ad-hoc `new Date().toISOString().slice(0, 10)` with `todayIso()` from `~/model/dates`. The raw form returns a UTC date and is off-by-one near midnight in non-UTC zones; `todayIso()` formats the local date.
- Add `Tomorrow` (`shiftIso(todayIso(), 1)`) and `Yesterday` (`shiftIso(todayIso(), -1)`), both calling `editor.chain().focus().deleteRange(range).insertDateChip(iso).run()`.

Order under Inline: Today → Tomorrow → Yesterday → Date (picker). Search terms include the keyword itself so typing `/tomorrow` filters directly to that item.

## File changes

```
calcifer/src/lib/tiptap-extension-slash-command/items.ts   ← add Tomorrow/Yesterday; switch Today to todayIso()
```

## Out of scope

- Free-form NL date parsing (`"next monday"`, `"+3d"`, etc.). Not on the roadmap; the `/Date` picker covers anything the three shortcuts don't.

## Verification

`pnpm dev`, then in a Note's body:

1. **`/today`** — slash menu surfaces Today; selecting inserts a chip whose iso matches today's local date.
2. **`/tomorrow`** — surfaces Tomorrow; chip iso is today + 1.
3. **`/yesterday`** — surfaces Yesterday; chip iso is today − 1.
4. **No regressions** — `/Date` still opens the picker; `/d` still filters across the four items.

## Definition of done

- Three keyword shortcuts ship under Inline.
- `Today` uses `todayIso()` and matches the user's local date around midnight.
