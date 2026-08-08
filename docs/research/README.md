# Research

Prior-art analysis of **other people's code**, captured while deciding how to build
Calcifer's editor.

| Document | Subject |
|---|---|
| [capacities-mentions.md](capacities-mentions.md) | How Capacities resolves `@` mentions at import time; the link data structure |
| [blocknote-vs-tiptap.md](blocknote-vs-tiptap.md) | BlockNote's slash menu and SideMenu vs TipTap's equivalents; popup DOM; drag-handle alignment |

## These files are frozen

They describe external codebases at a point in time (~2026-04). **Do not update them to
match Calcifer** — that's what [`../specs/`](../specs/) is for, and blurring the two is
what made the original combined document hard to read: you couldn't tell whether a code
block described our system or someone else's.

If the upstream projects change, these notes simply become dated. That's fine; their
value is the reasoning they fed into decisions already made.

Each document ends by recording what Calcifer took from it and what it dropped, which
is the part worth re-reading.
