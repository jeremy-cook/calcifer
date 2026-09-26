# Spec: Entity mentions (`@` and `#`)

How mentions work **as built**. Prior-art research that informed this is in
[`../research/capacities-mentions.md`](../research/capacities-mentions.md); the
graph semantics behind it are in
[ADR 3](../adr/0003-server-authoritative-link-graph.md).

**Code:** `calcifer/src/editors/tiptap/extensions/entityMention.ts`,
`components/mention/*`, `components/suggestionPopup.ts`.

---

## Nodes

Two node types, both extending `@tiptap/extension-mention`:

```ts
export const EntityMention = Mention.extend({ /* + structureType, char attrs */ })
export const HashtagMention = EntityMention.extend({ name: 'hashtag' })
```

`id` and `label` come from the parent extension. Two attributes are added:

| Attribute | Default | Serialized as |
|---|---|---|
| `structureType` | `null` | `data-structure-type` |
| `char` | `'@'` | `data-char` |

The server reads only `id` and `structureType`; see
[`../reference/richtext-doc.md`](../reference/richtext-doc.md).

Both render through `MentionNodeView` (a React node view), which is what makes a chip
clickable and lets deleted targets render as tombstones.

`#` is a **separate node type** (`hashtag`), not a `mention` with a different `char`.
The `char` attribute records the trigger for display; the node name is what
distinguishes them in the schema.

> Field naming: this is `structureType` (`structure_type` in proto), not the
> `structureId` used in the older research notes.

---

## Triggers

Both triggers are built by one factory, `makeSuggestion({ char, structureFilter? })`:

```ts
EntityMention.configure({ suggestion: makeSuggestion({ char: '@' }) })
HashtagMention.configure({ suggestion: makeSuggestion({ char: '#', structureFilter: 'Tag' }) })
```

- **`@`** — no structure filter. Searches entities whose Structure is `mentionable`.
- **`#`** — hard-filtered to `Tag`, so the trigger *is* the structure context.

### Query narrowing

`parseQuery(query)` splits on the first `/`:

- `@foo` → `{ structureType: null, term: 'foo' }` — search everything mentionable.
- `@Note/foo` → `{ structureType: 'Note', term: 'foo' }` — narrowed.
- `@Bogus/foo` → `{ structureType: null, term: 'Bogus/foo' }` — an unrecognised prefix
  is **not** an error; the whole string is treated as the search term.

The prefix is matched case-insensitively against Structure *names* in `STRUCTURE_LIST`.

### Result list

`items()` scans the entity snapshot and returns at most **8** matches. Filters applied
in order: structure narrowing → `mentionable` (bare `@` only) → case-insensitive
substring match on `name`.

The loop keeps scanning past the 8-item cap when it still needs to settle whether an
*exact* name match exists, because that gates the create item. It stops as soon as
both facts are known.

### Create-on-miss

A trailing "Create new …" item appears only when **all** of these hold:

1. A Structure is unambiguous — either narrowed via `@Structure/` or fixed by
   `structureFilter` (`#`).
2. The term is non-empty.
3. No existing entity matches the term exactly.
4. That Structure is `creatable` (so `DailyNote` never offers one).

Bare `@foo` therefore **never** creates. Creation requires explicit structure context
so a typo can't spawn an entity.

Label format: `Create new {StructureName}: {term}`.

---

## Insertion

`command()` resolves the item, then inserts:

```ts
editor.chain().focus()
  .deleteRange(range)
  .insertContent([
    { type: char === '#' ? 'hashtag' : 'mention',
      attrs: { id, label, structureType, char } },
    { type: 'text', text: ' ' },   // trailing space so typing continues naturally
  ])
  .run()
```

For a create item, resolution goes through `getOrCreateEntityForMention`, which calls
`EntityService.ResolveEntity` by name — **the server decides the id**. The browser does not
get-or-create locally, so the browser and the MCP agent converge on one entity for the
same name. This is why typing `#urgent` twice yields one Tag.

Insertion is async (it awaits the resolve), so the node appears on the next tick for
create items and immediately for existing ones.

**Links are not written here.** The chip is the only thing inserted; the outgoing
`LinkRef` is derived server-side from the document on save
([ADR 3](../adr/0003-server-authoritative-link-graph.md)).

---

## Popup

Both triggers share `createSuggestionPopup(MenuComponent)` with the slash menu — see
[`slash-menu.md`](slash-menu.md#popup) for the positioning and lifecycle details.
Mentions supply `MentionMenu` as the renderer.

---

## Known gaps

- **No paste/import resolution.** Pasting `@Structure/Entity/` text does not resolve to
  mentions; the research notes specced this and it was dropped.
- **No unresolved/pending chip state.** A mention always carries a real id, because
  creation resolves before insertion.
