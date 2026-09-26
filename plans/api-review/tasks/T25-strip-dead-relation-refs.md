# T25 · Deleting an entity strips it from other entities' relation values

**Area:** server, docs · **Issues:** I-14 · **Decisions:** D10, D11

## Background
`delete_entity` in `server/src/services/entity.rs` removes the entity's own rows and
every `links` row that points at it, but leaves its id in other entities' `relation` and
`relations` property values. The browser hides such a dead ref, but builds relation
edits from the raw list, so the dead ref is resent on every edit and can't be removed.
Relation link sync (`sync_relation_links` in `link_store.rs`) skips missing targets, so
this causes no error today. A stricter check later would lock such entities out of
`SetEntityProperty`. Read I-14 in `ISSUES.md`.

Every relation ref whose target exists has a `links` row with
`source_property_id` = the relation property's id (declared or ad hoc). Rich-text links
use the rich-text property's id (e.g. `content`).

## Requirements
- In `delete_entity`, in the same transaction, remove the deleted id from every other
  entity's `relation` and `relations` values:
  - `relations`: drop the matching refs. If none are left, delete the property row
    (an empty list is "cleared", as the browser's `setRelations` does).
  - `relation`: delete the property row.
  - Bump each changed entity's `updated_at`.
  - Find the affected (entity, property) pairs through `links` before the existing
    `DELETE FROM links WHERE target_id = ?` removes them. Decode only those property
    rows, and change only rows whose value is actually `relation` or `relations`
    (a rich-text link with the same target isn't a property value).
- After commit, publish `upserted` for every changed entity, as well as the existing
  `deleted_id`. Pick an order and state it in the doc comment.
- **D10:** the server strips refs on delete. No frontend change.
- **D11:** no cleanup code for dead refs that already exist (no startup sweep, no
  migration). The user handles the live DB's one existing dead ref outside the product.

## Tests
In `services/entity.rs`'s tests, e.g.:
- `delete_strips_the_id_from_relations_values`: a Todo tagged with two Tags; delete one;
  the Todo's `tags` holds only the other, and its `updated_at` moved.
- `delete_clears_a_relations_value_left_empty`: delete the only tag; the Todo has no
  `tags` property.
- `delete_clears_a_single_relation_value`: an ad-hoc `relation` property pointing at the
  deleted entity is gone.
- `delete_publishes_upserts_for_stripped_entities`: subscribe to the hub and see the
  changed Todo's `upserted` (with the stripped value) and the `deleted_id`.
- A rich-text mention of the deleted entity in another entity's `content` doesn't change
  that entity's properties.

## Out of scope
Frontend changes (D10). Existing dead refs in stored data (D11). Relation add/remove ops
(I-34). Rich-text chips that mention the deleted entity: the doc keeps them, and the
editor renders them as tombstones.

## Done when
- `cd server && cargo fmt --check` and `cargo test` pass.
- `docs/reference/data-model.md`: the relation paragraph no longer says a deleted target
  stays in stored values; it says `DeleteEntity` strips it and publishes upserts. The
  `DeleteEntity` description and the Watch event notes match.
- `ISSUES.md`: move I-14 to Resolved. Say that existing dead refs aren't cleaned up (D11).

## Commits
1. `server: strip a deleted entity from relation values (I-14)`
2. `docs: resolve I-14`
