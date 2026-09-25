# T13 · Batch-load entities: List and snapshot in constant queries

**Area:** server · **Issues:** I-27 (B3, the 1 + 4N part)

## Problem
`EntityService::list` hydrates each entity with `load_entity`, which is 1 query for the
row plus about 4 per entity (properties, links, referenced dates, …). So a full `List`
costs 1 + 4N queries. T14 will serve a full snapshot on every Watch connect, so this
needs to be constant-query first.

## Requirements
- Add `load_entities(pool, structure_type: Option<&str>) -> Vec<Entity>` in
  `server/src/services/entity.rs`, next to the existing `load_entity(pool, id)`:
  - one query for the entity rows;
  - one query each for properties, links and referenced dates, restricted by the same
    filter (a join or subquery on `entities`, not a giant `IN` list);
  - assemble in memory (e.g. `HashMap<id, Entity>`) and return in the same order `list`
    returns today.
- `list` uses it. `load_entity` either stays as is or becomes a thin wrapper; your call.
  Don't change any other caller.
- Output must be identical to today's, field for field, including the ordering of
  `properties`, `links` and `referenced_dates` inside an entity. Check how `load_entity`
  orders them and match it.

## Tests
- `load_entities_matches_load_entity`: seed several entities of different structures
  with properties, relation links, rich-text links (via `RichTextService.Put`) and
  referenced dates. Assert that `load_entities(None)` equals `[load_entity(id) for each]`
  in order, and do the same with a structure filter.

## Out of scope
Watch (T14). Any proto change. Other N+1s (mention them in your report).

## Done when
- `List` costs a constant number of queries regardless of N. Say how you checked this,
  e.g. by reading the code path.
- `cargo test` passes.

## Commit
`server: load entity lists in constant queries (I-27)`
