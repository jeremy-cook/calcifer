# Research: How Capacities resolves `@` mentions

> **Frozen — historical.** Reverse-engineering notes from ~2026-04, taken from
> `~/Capacities/electron-dist/shared-logic/util/` before Calcifer's mention system was
> built. This describes **Capacities' code, not ours.** It is not maintained and should
> not be updated to match Calcifer. For what Calcifer actually does, see
> [`../specs/mentions.md`](../specs/mentions.md).
>
> Naming here is Capacities': `structureId`, `RootTag`, `UtilDate`. Calcifer uses
> `structure_type`/`structureType`, `Tag`, and no date entity at all.

---

## Why this was worth reading

Capacities is a production app with the same core idea — typed entities linked from
prose. The question was how it turns text into resolved, typed references, and which of
its choices to keep.

The headline finding: Capacities resolves mentions by **batch regex scan at import
time**, not through a real-time suggestion popup. That's a fundamentally different
architecture from what a live editor needs, so most of the pipeline wasn't reusable —
but the *data shape* of a resolved link was.

## The pipeline

```
Raw text/markdown
      ↓
  mdast parser (mdast-util-from-markdown + GFM plugins)
      ↓
  AST traversal (traverseASTRecursive)
      ↓
  paragraph node → paragraphToTokens()
      ↓  [for each 'text' AST node]
  linkEntitiesInText()    ← importBackbone.js
      ↓  [loops while matches exist]
  matchEntityLink()       ← regex scan of raw text
      ↓  [per match]
  matchStructure()        ← structureTitle → structureId
  findEntity()            ← (title, structureIds) → component
  generateEntity()        ← if context='create' and not found
      ↓  [if resolved]
  generateInternalEntityLink()   → "[label](NOTE/entityId)" markdown
      ↓
  mdToBlocksBackbone()    ← re-parse the replaced text
      ↓
  link node → generateLinkNode()
      ↓
  LinkToken { entity: { id, link: { data: { toStructureId } } } }
```

## Pattern detection

`importBackbone.js` lines 56–112. Two regexes in priority order.

**Regex 1** (mention-style):
`/(?:(\+|\/|@))([^\s\/]+)(?:\/)([^\/]+)(?:\/)/gm`

| Prefix | Context | Syntax |
|---|---|---|
| `@` | `'reuse'` — link existing entity only | `@StructureTitle/EntityTitle/` |
| `/` | `'create'` — create if not found | `/StructureTitle/EntityTitle/` |
| `+` | `'create'` — create if not found | `+StructureTitle/EntityTitle/` |

**Regex 2** (wikilink-style, fallback): `/\[\[([^\]\[]+)\]\]/gm`

Always `context: 'reuse'`. Accepts `[[EntityTitle]]` or `[[StructureTitle/EntityTitle]]`.

Returns:

```ts
{
  context: 'reuse' | 'create',
  structureTitle: string | undefined,
  entityTitle: string,
  matchObject: { text, offset, length }
}
```

## Resolution loop

`importBackbone.js` lines 192–268, per text node:

```
while (entityMatch = matchEntityLink(text)):

  1. Special case: structureTitle === 'NOTE'
       → loadAndGetC(entityTitle)   // entityTitle is a raw id
       → skip normal lookup

  2. matchStructure(structureTitle) → structureId
       → if not found and a structureTitle was given: skip this match

  3. findEntity(entityTitle, { filterStructureIds, excludeIds })

  4. If not found AND context='create' AND structureId resolved:
       → generateEntity(structure, ...)
       → component.properties.title = entityTitle

  5. Special case: structureId === 'UtilDate'
       → parseNaturalLanguageDate(entityTitle)

  6. If resolved:
       → text = text.slice(0, offset)
               + "[entityTitle](NOTE/entityId)"
               + text.slice(offset + length)
       → loop continues on the modified text
```

The rewritten text is re-parsed by `mdToBlocksBackbone()`, turning
`[label](NOTE/id)` into `link` AST nodes.

## The resolved link structure

`componentHelper.js` lines 188–208:

```ts
function generateLinkNode(toEntity, type, propertyId) {
  return {
    id: toEntity.id,           // entity being referenced
    link: {
      id: uuid(),              // unique id for this relationship instance
      type,                    // 'Dependency' (inline) or 'Database' (property)
      createdAt: ISO_string,
      data: {
        toStructureId: toEntity.structureId,
        propertyId,            // set for property-specific links
      },
    },
  }
}
```

All inline `@mention` links use `type: 'Dependency'`. The full token:

```ts
{
  type: 'LinkToken',
  id: uuid(),
  text: 'Alice',
  entity: generateLinkNode({ id: 'abc123', structureId: 'Person' }, 'Dependency', undefined),
  url: undefined,
}
```

## Special cases

- **`NOTE/` prefix** — `structureTitle === 'NOTE'` means the entity title is a raw id,
  loaded directly. This is how already-resolved links survive repeated passes through
  the text processor.
- **`UtilDate`** — a structure whose entity title is a natural-language date string
  (`"next Monday"`, `"2024-01-15"`), parsed into a date entity.
- **`allowDailyNoteLinking`** — a flag gating whether `RootDailyNote` entities may be
  linked. Off in most contexts.

---

## What Calcifer took, and what it dropped

**Taken:**

- **Two-part entity references.** Capacities stores `entityId` *and* `toStructureId`;
  TipTap's stock mention node stores only a flat `id`. Calcifer's node carries
  `structureType` for the same reason — you cannot render or route a chip correctly
  from an id alone.
- **A relationship id distinct from the target id.** Calcifer's `LinkRef.id` is a uuid
  per link instance, mirroring `entity.link.id`.
- **`allowDailyNoteLinking`, generalised.** Calcifer's per-Structure `mentionable` flag
  is the same idea, applied to every Structure rather than hardcoded to daily notes.

**Dropped:**

- **`[[wikilink]]` syntax in the editor.** It is functionally identical to `@` — same
  `context: 'reuse'`, same resolved token — so it was redundant complexity. (It *is*
  supported on the markdown ingest path in `mcp-server/src/markdown/`, where agents
  write it.)
- **The `+` prefix.** Duplicated `/`.
- **`/Structure/Entity/` as a create syntax.** `/` became the block-insertion slash
  menu instead; creation flows through `@Structure/term` and `#tag`.
- **Import-time batch resolution.** Calcifer resolves in real time via the suggestion
  popup; there is no paste-resolution path.
- **`UtilDate`.** Calcifer has no date entity — see
  [ADR 4](../adr/0004-dates-without-a-dateref-entity.md).
- **`NOTE/entityId` round-trip format.** Unnecessary without a text-rewriting pass.
