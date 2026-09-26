# T24 · `cargo fmt`, then request guards (Resolve, ordering, empty ids, dates)

**Area:** server, docs · **Issues:** I-41, I-46, I-48, I-50, I-52 (server half)

## Background
Five small server defects, found during the API review. Read each entry in `ISSUES.md`.
Entries written before 2026-09-26 use old RPC names; the note at the top of "Open" maps them.

- **I-41:** `cargo fmt --check` fails in `db.rs`, `embed/chunk.rs`, `link_store.rs`,
  `services/entity.rs`, `services/richtext.rs`, `services/search.rs` and
  `services/structure.rs`. The module comment at the top of `links.rs` points at
  `extractDocReferences` in `calcifer/src/model/linkSync.ts`, which no longer exists.
- **I-46:** `resolve_name` in `services/entity.rs` creates an entity of any known
  structure when `create_if_missing` is set, including a DailyNote (`creatable = false`),
  which then has no date. `create_entity` already refuses non-creatable structures.
- **I-48:** `load_entity` reads links and referenced dates with no `ORDER BY`, while
  `load_entities` orders them explicitly (`ORDER BY entity_id, link_id` and
  `entity_id, iso_date`). The list queries in `load_entities` order by `updated_at DESC`
  with no tie-breaker. A comment above them says they keep the plan "that List has always
  had"; that reasoning goes away with this fix.
- **I-50:** `get_rich_text` and `put_rich_text` in `services/richtext.rs` pass an empty
  `entity_id` straight to `check_declared`, which answers `NOT_FOUND` "entity ".
- **I-52 (server half):** no `date` property value is format-checked in `CreateEntity`,
  `ResolveEntity` or `SetEntityProperty`; only `ResolveEntity`'s date key is (it parses
  with `chrono::NaiveDate` and requires the canonical form). A `dateChip`'s `date` goes
  into `referenced_dates` unchecked (`walk` in `links.rs`). A non-ISO DailyNote `date`
  becomes its `date_key`, and its name falls back to the raw string.

## Requirements
- **I-41, first and alone:** run `cd server && cargo fmt` and commit only that, before
  any other change. Then fix the `links.rs` module comment so it describes the module
  as it is now (it's the only implementation; there's no frontend mirror).
- **I-46:** in `resolve_name`, when the name isn't found and `create_if_missing` is set,
  return `FAILED_PRECONDITION` if the structure isn't `creatable`. For DailyNote, say to
  resolve by `date` instead, matching `create_entity`'s message. A lookup (found, or
  missing without `create_if_missing`) behaves as before.
- **I-48:** give `load_entity` the same explicit orders as `load_entities` for links and
  referenced dates, so both loaders return the same entity for the same rows. Add `, id`
  to every `ORDER BY updated_at DESC` in `load_entities` and wherever else an entity list
  is ordered that way (grep for it, including the test helpers). Update the comment above
  the list queries.
- **I-50:** in both `get_rich_text` and `put_rich_text`, return `INVALID_ARGUMENT` for an
  empty `entity_id` or `property_id` before any lookup.
- **I-52:**
  - Every `date` property value must be a real calendar day in canonical `yyyy-MM-dd`
    form, whether the property is declared or ad hoc, in `CreateEntity`, `ResolveEntity`
    and `SetEntityProperty`: otherwise `INVALID_ARGUMENT`. An empty string is invalid
    too; clients clear a date by removing the property. Put the check where all three
    paths pass (`validate_property` is the obvious place, but note its early returns for
    unknown structures and ad-hoc ids).
  - Share one "is this a canonical ISO day" helper with `resolve_date`'s existing check,
    rather than writing a second parser.
  - In `links.rs`, skip a `dateChip` whose `date` isn't a canonical ISO day, as a missing
    `date` is skipped today.
  - Don't edit `proto/`. Its comment "The server doesn't check the format" on
    `PropertyValue.date` is fixed in a later task that regenerates stubs.

## Tests
In the `#[cfg(test)]` modules, named for the behaviour, e.g.:
- `resolve_by_name_of_a_missing_daily_note_is_failed_precondition` (and a lookup without
  create still answers `NOT_FOUND`).
- `load_entity_matches_load_entities` for an entity with several links and dates;
  `list_ties_are_broken_by_id` for entities with equal `updated_at`.
- `rich_text_with_an_empty_id_is_invalid_argument` covering both RPCs and both ids.
- `date_values_must_be_iso_days`: `2026-13-45`, `2026-2-3`, `June 13` and `""` are
  rejected by `CreateEntity` and `SetEntityProperty` (declared and ad-hoc properties);
  `2026-02-03` is accepted.
- A `links.rs` test: a doc with a valid and an invalid `dateChip` yields only the valid
  date.

## Out of scope
Changing `proto/` (see above). Content link types and search text (I-51, I-54; a later
task). Relation dead refs (I-14). Concurrency (D9).

## Done when
- `cd server && cargo fmt --check` and `cargo test` pass.
- The first commit is exactly `cargo fmt`'s output and nothing else.
- `docs/reference/richtext-doc.md` (the `dateChip` section) and
  `docs/reference/data-model.md` (wherever they say dates aren't checked) describe the
  new rules.
- `ISSUES.md`: move I-41, I-46, I-48 and I-50 to Resolved. Leave I-52 open, and add a
  line to it saying that the server half is done (the MCP parser and the proto comment
  are still open).

## Commits
1. `server: apply cargo fmt (I-41)`
2. `server: fix the stale links.rs module comment (I-41)`
3. `server: refuse creating a non-creatable structure through ResolveEntity (I-46)`
4. `server: order entity loads and lists explicitly (I-48)`
5. `server: reject empty rich-text ids as INVALID_ARGUMENT (I-50)`
6. `server: accept only ISO days in date values and date chips (I-52)`
7. `docs: document date checks; resolve I-41, I-46, I-48, I-50`
