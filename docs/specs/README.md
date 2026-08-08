# Specs

How built features actually work.

| Spec | Covers |
|---|---|
| [mentions.md](mentions.md) | `@` and `#` entity mentions — nodes, triggers, query narrowing, create-on-miss, insertion |
| [slash-menu.md](slash-menu.md) | `/` block insertion, item registry, and the suggestion popup shared with mentions |
| [drag-handle.md](drag-handle.md) | Gutter alignment and block reordering |

## The rule

**A spec describes what exists — never what's planned.**

Write or update one *when a feature ships*, describing the thing that actually got
built. If it disagrees with the code, the spec is wrong and should be fixed.

This is deliberate. The predecessor of this directory was a set of design-ahead plans
written *before* the work: they were never updated when reality diverged and never
deleted when the work shipped, so they rotted into confidently wrong documentation. A
spec that can only be written after the fact can't fail that way.

Each spec ends with a **Known gaps** section — the places the implementation
deliberately or accidentally falls short of what was designed. That's the honest part;
keep it current.

## Related

- [`../adr/`](../adr/) — *why* a decision was made (immutable)
- [`../research/`](../research/) — external prior art (frozen, describes other people's code)
- [`../reference/data-model.md`](../reference/data-model.md) — cross-cutting shapes
- [`../../ROADMAP.md`](../../ROADMAP.md) — what isn't built yet
