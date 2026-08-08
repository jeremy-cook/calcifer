# Docs

Four kinds of document, separated because they have different lifecycles. Knowing which
kind you're holding tells you how much to trust it and whether you may edit it.

| Directory | Answers | Lifecycle |
|---|---|---|
| [`adr/`](adr/) | *Why* did we decide this? | **Immutable.** Supersede with a new record; never edit. |
| [`reference/`](reference/) | What are the cross-cutting shapes? | Tracks the code. Update when the model changes. |
| [`specs/`](specs/) | How does this built feature work? | Tracks the code. Written when a feature ships. |
| [`research/`](research/) | How does *someone else's* system work? | **Frozen.** Dated snapshots of external code. |

Plus [`../ROADMAP.md`](../ROADMAP.md) for what isn't built yet, and
[`../README.md`](../README.md) for orientation and how to run things.

## The lifecycle

```
ROADMAP entry (intent, a paragraph)
        ↓  built
ADR if a real decision was made  +  spec describing what now exists
        ↓
ROADMAP entry deleted
```

**Specs describe what exists, never what's planned.** This is the rule that keeps the
directory honest, and it exists because of a specific failure: the predecessor to these
docs was ~22 design-ahead plan files written before the work. They were never updated
when the implementation diverged and never deleted when it shipped, so they accumulated
into confidently wrong documentation — one still listed unchecked boxes for work
finished months earlier. A document that can only be written *after* the fact can't rot
that way.

The corollary: if a roadmap item needs enough design detail to warrant its own file,
that's a signal it's ready to be built, not that it needs a plan document.

## Conventions

- Specs end with a **Known gaps** section — where the implementation falls short of
  what was designed, deliberately or not. Keep it current; it's the most useful part.
- Research documents end with **what we took and what we dropped**.
- Don't update a research file to match Calcifer. Write or fix a spec instead.
