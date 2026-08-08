# 6. Drop the TipTap-vs-Lexical comparison; standardise on TipTap

- **Status:** Accepted — supersedes the editor comparison plan
- **Date:** 2026-08-08 (recognised during doc consolidation; de facto much earlier)

## Context

Calcifer began partly as a case study: implement the same editor features twice, once
in TipTap and once in Lexical, behind a tab switcher on the home page, and use the
Lexical playground as the feature ceiling. A 22-phase plan and a 60-row parity matrix
tracked both columns.

That never happened. Lexical was never added as a dependency and no Lexical editor was
ever written. `src/editors/` has only ever contained `tiptap/`.

Meanwhile the app outgrew the premise. Once the home page became an entity page and the
editor became the surface for mentions, entity links, date chips, and the slash menu,
the editor stopped being a swappable component to evaluate and became the substrate the
knowledge model is expressed through. A parallel Lexical implementation would have had
to reimplement entity mentions, `ResolveByName` integration, date chips, and
content-derived link extraction to stay comparable — for a comparison nobody was
running.

The parity matrix meanwhile rotted into actively false documentation: every one of its
60 rows still read "not started" while links, tables, images, mentions, the slash menu,
and drag-handle reordering had all shipped.

## Decision

Abandon the comparison. TipTap is the editor, not a candidate.

Delete the comparison plan rather than leaving it as aspirational tracking. Keep the
Lexical playground as an informal feature ceiling — a source of ideas for what a rich
editor can do — with no expectation of parity and no dual implementation.

Editor feature status is recorded as prose in the README instead of a per-feature
matrix.

## Consequences

- No obligation to hold a feature back, or shape an abstraction, so that a second
  editor could match it. Entity mentions can bind directly to TipTap's suggestion
  plugin and node views.
- The DX comparison this project was partly meant to produce will not be produced.
- Editor choice is now effectively permanent. Migrating off TipTap would mean rewriting
  the mention extensions, the slash menu, date chips, node views, and the TipTap-JSON
  document format that the server parses for links and full-text extraction.
- TipTap Pro gating is a live constraint rather than a comparison footnote: drag-handle,
  mathematics, details, and table-of-contents are Pro extensions. Community or custom
  implementations are used where needed.
- A narrative status section can go stale without being *wrong* per-row, which the
  matrix could not. It is also faster to keep honest.
