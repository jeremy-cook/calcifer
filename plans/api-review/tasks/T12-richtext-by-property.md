# T12 · Address rich text by declared property; drop stored RichTextRefs

**Area:** proto, server, fe, mcp, docs · **Issues:** closes I-22 (A3)

## Background
A rich-text document is identified by `(entity id, property id)`. The structure registry
(`StructureDef.properties` with `kind === PropertyKind.RICHTEXT`) already says which
properties are rich text. Today every new entity also stores a
`PropertyValue { richtext: RichTextRef }` for each rich-text property, and the FE reads
that stored value to find the document, so it renders nothing if the row is missing
(ADR 8 says to stop).

Proto changes in this plan **break in place** (decision D4): one task changes the proto
and every consumer together, and deletes what it replaces. Intermediate commits inside
this task may leave a consumer uncompiled, but the branch must build, lint, typecheck
and pass tests when you report.

**Gate G2:** your prompt must say the user approved decision **D5** (deleting stored
rich-text property rows). If it doesn't, stop and report.

## Current code
- **Server** (`server/src/services/entity.rs`): `new_entity` writes a `RichTextRef`
  value for each rich-text property. The kind check (I-16) lets rich-text-kind
  properties accept only the `richtext` case.
- **FE** (`calcifer/src/`):
  - `routes/e.$id.tsx`, `EntityProperties` → `renderProperty`, the
    `PropertyKind.RICHTEXT` case (about l.209): `if (value?.case !== 'richtext') return null`.
  - `components/calendar/DailyNoteSection.tsx`: `contentRichTextRef` (about l.148),
    used by `DailyNoteBody` (about l.91) and the prune path.
  - Anything else found by `grep -rn "'richtext'" calcifer/src`.
- **MCP** (`mcp-server/src/`): check for any read of the `richtext` value case.

## Proto
- `PropertyValue`: remove `RichTextRef richtext = 6`, and add `reserved 6; reserved
  "richtext";`. `RichTextRef` stays as `RichTextService`'s address.
- Regenerate both TS stubs.

## Server
- `new_entity` no longer writes rich-text property values.
- `SetProperty` and Create reject any value for a rich-text-kind property with
  `InvalidArgument`.
- **Migration** (`server/migrations/<timestamp>_drop_richtext_property_values.sql`):
  delete the `properties` rows that hold rich-text refs. `value_blob` is prost-encoded,
  so select rows by the registry instead: `(structure_type, property_id)` pairs whose
  kind is RICHTEXT (today `content` on Note, DailyNote and Todo; confirm in
  `structures.rs`). Join `entities`. Add a comment naming the source of the list.
- Build and test against a scratch DB, as the engineer brief describes for migrations.
  Never run it against `server/calcifer.db`; the tech lead does that after a backup.
- Tests: new entities have no rich-text property value; a rich-text value on Create or
  SetProperty is rejected; the migration deletes exactly the rich-text rows (seed a
  scratch pool with old-style rows if practical, or explain how you checked).

## FE
- Add one model helper, e.g. `richTextRef(entityId, propertyId): RichTextRef` in
  `model/richtext.ts`. Build it with the proto `create` there, so components don't
  import `@bufbuild/protobuf` for it.
- Every rich-text editor mount derives its ref from the entity id and the **declared**
  property id. It renders for every declared rich-text property; no property value is
  involved.
- `DailyNoteSection` gets the content ref the same way (`content` is the declared
  rich-text property of DailyNote; read it from the registry rather than hard-coding it,
  if that's natural).
- Follow `CLAUDE.md` component style.

## MCP
Fix any fallout from the removed case. If there's none, say so.

## Docs
- `docs/reference/data-model.md`: remove the `richtext` value case and describe
  addressing by declared property.
- `ISSUES.md`: move I-22 to Resolved (house format).

## Out of scope
Watch (T14). Relation dead refs (I-14). Unused kinds (T23). Other proto-leak cleanups
(T21).

## Done when
- `grep -rn "case: 'richtext'\|Value::Richtext\|richtext = 6" calcifer/src mcp-server/src server/src proto --exclude-dir=gen`
  finds nothing, and `grep -rn "'richtext'" calcifer/src` finds only query keys or kind
  references.
- `cargo test` passes against the scratch DB, `calcifer` builds and lints, and
  `mcp-server` typechecks.
- Your report gives the tech lead the migration's exact SQL and a read-only query to
  count the rows it will delete from the live DB.
- Browser checks to report: existing notes, todos and daily notes open with their
  content, and new ones get a working editor.

## Commits
1. `proto: remove the stored richtext property value (I-22)`, with regenerated stubs.
2. `server: stop storing rich-text refs as property values (I-22)`
3. `server: migrate away stored rich-text property values (I-22)`
4. `fe: address rich-text docs by declared property, not stored ref (I-22)`
5. `mcp: …` only if there was fallout.
6. `docs: resolve I-22`
