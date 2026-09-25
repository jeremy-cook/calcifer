# T06 · FE: live rich text, conflict handling, safe prune

**Area:** fe · **Issues:** I-17 (B1), I-18 (B2), I-35 (P2)

## Background
The server (T04) now:
- publishes `EntityEvent.rich_text_changed` (the saved `RichText`: ref, doc,
  `updated_at`) on every Put;
- supports `RichText.expected_updated_at`: unset means unconditional, a mismatch means
  `FAILED_PRECONDITION`, and the epoch means "nothing saved yet";
- makes `updated_at` strictly increasing per doc;
- has `Get` return an empty doc with epoch `updated_at` when nothing is saved.

See the comments in `proto/calcifer/v1/`.

Your prompt includes decision **D3**, which covers what the browser does when its save
loses a conflict. Recommended: reload the server's version and show a notice.

## Problems
1. `EntityRichTextField` mounts `TiptapEditor` with `doc` as initial content only, so an
   agent's append never shows up in an open editor. The next debounced save overwrites
   it.
2. The cache (`qk.richtext`) holds only the doc string, so the FE can't send
   `expected_updated_at`.
3. `DailyNoteSection`'s unmount effect deletes the day's note when the *cached* doc is
   empty. A stale cache can delete content the agent appended (I-35).

## Files
`calcifer/src/model/richtext.ts`, `calcifer/src/App.tsx` (Watch loop: add one case
only), `calcifer/src/components/entity/EntityRichTextField.tsx`,
`calcifer/src/editors/tiptap/TiptapEditor.tsx`,
`calcifer/src/components/calendar/DailyNoteSection.tsx`, and `store.ts` only if the
prune helper lives there.

## Requirements
- **Cache shape:** `qk.richtext` holds `{ doc: string, updatedAt: Timestamp | undefined }`.
  Update `useRichText`, `getRichTextSnapshot` and all readers. Remove the `isNotFound`
  fallback in `useRichText`.
- **Watch:** handle `rich_text_changed` in the existing loop in `App.tsx`. Write it to
  `qk.richtext(entityId, propertyId)` only if its `updatedAt` is newer than the cached
  one. Keep the change to the loop minimal; T15 rewrites it.
- **Saving:** `usePutRichText` sends `expectedUpdatedAt` from the cache (epoch if none).
  - Saves for one doc are serialised: while a Put is in flight, keep only the latest
    pending doc and send it after the in-flight Put resolves, using the new
    `updatedAt`.
  - On success, write the response to the cache.
- **Outside changes into the editor:**
  - `TiptapEditor` gets a way to replace its content without emitting `onUpdate`. For
    example, an `externalDoc` plus version prop applied in an effect; check the
    installed TipTap's `setContent` signature.
  - `EntityRichTextField` applies a cached doc whose `updatedAt` is newer than the
    editor's own last-known version, **only when no local save is pending or in
    flight**.
  - Keep the cursor where it was if it's still valid, else put it at the end.
- **Conflict (D3):** on `FailedPrecondition`:
  - refetch the doc;
  - update the cache;
  - replace the editor content;
  - show a small inline notice near the editor, e.g. "Updated elsewhere; your latest
    edit wasn't saved", that clears on the next edit.

  No toast library exists; don't add one.
- **Prune (I-35):** before deleting an empty daily note, fetch the doc from the server
  through a model helper (not the cache), and delete only if it's empty. Add a one-line
  comment noting the remaining small race.
- Follow `CLAUDE.md` component style.

## Out of scope
- Removing the `['entities']` invalidations (T15).
- Moving the Watch loop out of `App.tsx` (T15).
- How rich text is addressed (T11).
- Merging concurrent edits.

## Done when
- `pnpm build` and `pnpm lint` pass.
- No code path treats `NotFound` as "empty doc".
- Your report lists browser checks for the tech lead:
  1. Open a note, then append to it via the MCP `append_to_note`. The text appears
     without a reload, and typing afterwards keeps it.
  2. Type continuously during an agent append. The conflict notice appears and the
     agent's text is present.
  3. Create a daily note, leave it empty, and navigate away. It's pruned.
  4. Create a daily note, have the agent append to that date, and navigate away. It's
     kept.

## Commits
1. `fe: apply outside rich-text changes and send expected_updated_at (I-17, I-18)`
2. `fe: re-check the server before pruning an empty daily note (I-35)`
3. `docs: resolve I-17, I-18 and I-35 in ISSUES.md`. Resolve I-17 and I-18 only if T05
   has landed (check `git log`); otherwise resolve I-35 alone and say so in your report.
