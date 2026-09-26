# T06b · FE: flush a pending rich-text save before pruning a daily note

**Area:** fe · **Issues:** I-39

## Background
- `LiveRichTextEditor` (`calcifer/src/components/entity/EntityRichTextField.tsx`)
  saves 300 ms after the last keystroke (`RICHTEXT_DEBOUNCE_MS`).
- Saves go through `usePutRichText` (`calcifer/src/model/richtext.ts`), which
  serialises them per editor. Its queue lives in a `useRef`.
- `DailyNoteSection`'s effect cleanup (`calcifer/src/components/calendar/DailyNoteSection.tsx`)
  prunes the day's note when you leave the day. It calls
  `deleteEntityIfRichTextEmpty` (`calcifer/src/model/store.ts`), which fetches the doc
  from the server and deletes the entity if the doc is empty.

## Problem (I-39)
If you type into a new, empty daily note and leave the day within 300 ms, the prune's
server check still sees an empty doc and deletes the note. The debounced save then
fires against a deleted entity and fails, and the typed text is lost.

## Requirements
- **Flush on unmount:** when `LiveRichTextEditor` unmounts with a debounced save
  pending, clear the timer and send that save straight away, with the latest doc (keep
  it in a ref). An unmount with nothing pending sends nothing.
- **Wait for saves before pruning:**
  - In `model/richtext.ts`, track outstanding saves per doc (key them with
    `richTextKey`). A save is outstanding while it's in flight or queued.
  - Expose a helper that resolves once the doc has no outstanding saves, e.g.
    `whenRichTextSaved(ref): Promise<void>`.
  - `deleteEntityIfRichTextEmpty` awaits it before its server check.
- **Cleanup order:** React may run `DailyNoteSection`'s cleanup before or after the
  editor's unmount cleanup. The fix must work either way. For example, have the prune
  defer to the next macrotask (`setTimeout(…, 0)`) before it checks for outstanding
  saves. Say in a comment why.
- Keep the existing one-line comment about the remaining small race (a write that
  lands between the Get and the Delete).
- Don't change the save, conflict or Watch behaviour from T06.
- Follow `CLAUDE.md` component style.

## Out of scope
- Merging edits.
- Server-side protection against deleting a non-empty note.
- The per-editor (not per-doc) save queue. Tracking outstanding saves per doc for the
  prune is enough.

## Done when
- `pnpm build` and `pnpm lint` pass.
- Your report lists browser checks for the tech lead:
  1. On a day with no daily note, click New, type a few characters, and within 300 ms
     click "Next day". Come back: the note exists with the typed text.
  2. Create a daily note, leave it empty, and go to the next day. It's pruned (T06
     behaviour kept).
  3. In a note, type and navigate away within 300 ms. The text is saved (check with a
     reload).
- Don't edit `ISSUES.md`; another task runs in parallel, and the tech lead resolves
  the issue at integration.

## Commits
`fe: flush pending rich-text saves before pruning a daily note (I-39)`
