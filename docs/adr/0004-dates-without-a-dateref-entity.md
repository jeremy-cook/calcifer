# 4. Dates are a calendar surface, not an entity

- **Status:** Accepted — supersedes the original two-Structure sketch
- **Date:** 2026-05-08 (landed in `2630d21`)

## Context

Dates need to be first-class: a page per day, a journal entry per day, and "what
references this day?" The original design had two cooperating Structures — `DateRef`,
a date-as-entity that acts as a backlink magnet, and `DailyNote`, the day's journal,
linked to its `DateRef`. Earlier notes called the first one `UtilDate`.

Making a date an entity is tempting because it reuses the whole mention machinery for
free: a date chip becomes an ordinary `LinkRef`, and "references to this day" becomes
an ordinary backlinks query.

The cost shows up in entity churn. Typing a date chip would create a `DateRef`;
deleting the chip would leave an orphan, or require reference-counted cleanup. Every
insert/delete cycle while drafting a sentence would spawn and strand entities. It also
forces a question with no good answer: is a day that nobody has written about still an
entity? Rendering a calendar means either materialising every day or special-casing
the gaps.

## Decision

**No date entity.** Not `DateRef`, not `UtilDate`.

- `DailyNote` is a Structure that carries its own `date` property (`"yyyy-MM-dd"`, a
  calendar day, not an instant) and sets its `name` to the long-form date at creation.
- Date chips in rich text are cosmetic nodes. Clicking one navigates to
  `/calendar?date=$iso`.
- References to a day are recorded as `referenced_dates: string[]` on the **source**
  entity, reconciled on save alongside links (see
  [ADR 3](0003-server-authoritative-link-graph.md)).
- **The calendar page is the page for a date.** There is no `/e/$id` for a day.

## Consequences

- Inserting and deleting date chips creates no entities and strands nothing.
- The calendar renders any day, written-about or not, without materialising rows.
- Mini-calendar content markers and the references panel are pure derivations over
  `referenced_dates` and existing `DailyNote`s — no extra persisted state.
- Dates do **not** get backlinks for free; the references panel is a separate query
  path (`entitiesByDate`) rather than the shared backlinks component.
- Date chips are not `LinkRef`s, so they don't appear in `Entity.links` and aren't
  navigable to an entity page. Two reference systems now exist side by side.
- `DailyNote` needs uniqueness per day, enforced by a `date_key` column and the
  `one_daily_note_per_day` index, with unique-violation mapped to `ALREADY_EXISTS`.
  Moving a daily note's date has to refuse when the target day is taken.
- Empty `DailyNote`s are auto-pruned on navigation away, because lazy click-to-create
  otherwise litters the store.
