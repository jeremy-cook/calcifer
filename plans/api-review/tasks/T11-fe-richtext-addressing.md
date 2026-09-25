# T11 · FE: address rich text by registry, not by property value

**Area:** fe · **Issues:** I-22 (A3)

## Background
A rich-text document is identified by `(entity id, property id)`. The structure registry
(`useStructure` / `StructureDef.properties` with `kind === PropertyKind.RICHTEXT`) already
says which properties are rich text. Today the FE instead reads a stored
`PropertyValue { case: 'richtext', value: RichTextRef }`, and renders nothing if that row
is missing. T12 removes the `richtext` case from the proto and deletes the stored rows,
so after this task nothing on the FE may depend on them.

## Current code
- `calcifer/src/routes/e.$id.tsx`, `EntityProperties` → `renderProperty`, the
  `PropertyKind.RICHTEXT` case (about l.209): `if (value?.case !== 'richtext') return null`.
- `calcifer/src/components/calendar/DailyNoteSection.tsx`: `contentRichTextRef` (about
  l.148), also used by the prune path.
- Anything else found by `grep -rn "'richtext'" calcifer/src`.

## Requirements
- Add one model helper, e.g. `richTextRef(entityId, propertyId): RichTextRef` in
  `model/richtext.ts`. Build it with the proto `create` there, so components don't
  import `@bufbuild/protobuf` for it.
- Every rich-text editor mount derives its ref from the entity id and the **declared**
  property id. It renders for every declared rich-text property, whether or not a
  property value exists.
- `DailyNoteSection` gets the content ref the same way (`content` is the declared
  rich-text property of DailyNote; read it from the registry rather than hard-coding it,
  if that's natural).
- Afterwards, `grep -rn "'richtext'" calcifer/src` finds only registry or kind
  references, not `PropertyValue` case checks.

## Out of scope
Server or proto changes (T12). Other proto-leak cleanups (T21).

## Done when
- The grep is clean, and `pnpm build` and `pnpm lint` pass.
- Browser checks to report: existing notes, todos and daily notes open with their
  content, and new ones get a working editor.

## Commit
`fe: address rich-text docs by declared property, not stored ref (I-22)`
