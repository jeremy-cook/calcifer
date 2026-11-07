use std::collections::HashMap;

use sqlx::SqlitePool;

use super::lookup::sort_facets;
use super::mask::ReadMask;
use super::EntityService;
use crate::error::AppError;
use calcifer::v1::{Entity, EntityRef, LinkRef, PropertyValue};

impl EntityService {
    /// Hydrate a full Entity (metadata + properties + links + referenced_dates).
    /// Shared by every RPC that returns an entity, so the read shape is defined once.
    pub(super) async fn load_entity(&self, id: &str) -> Result<Entity, AppError> {
        load_entity(&self.pool, id).await
    }
}

/// Hydrate a full Entity (metadata + properties + links + referenced_dates) from
/// the pool. The single read-shape definition, shared by EntityService and
/// SearchService so both services return identical entities.
pub(crate) async fn load_entity(pool: &SqlitePool, id: &str) -> Result<Entity, AppError> {
    let row = sqlx::query!(
        r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                  created_at AS "created_at!", updated_at AS "updated_at!"
           FROM entities WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("entity {id}")))?;

    let prop_rows = sqlx::query!(
        r#"SELECT property_id AS "property_id!", value_blob AS "value_blob!"
           FROM properties WHERE entity_id = ?"#,
        id
    )
    .fetch_all(pool)
    .await?;

    let properties = prop_rows
        .into_iter()
        .map(|r| {
            let value: PropertyValue = prost::Message::decode(&*r.value_blob)?;
            Ok::<_, AppError>((r.property_id, value))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    let link_rows = sqlx::query!(
        r#"SELECT link_id AS "link_id!", target_id AS "target_id!",
                  target_structure AS "target_structure!",
                  source_property_id AS "source_property_id!", created_at AS "created_at!"
           FROM links WHERE entity_id = ? ORDER BY link_id"#,
        id
    )
    .fetch_all(pool)
    .await?;

    let links = link_rows
        .into_iter()
        .map(|r| LinkRef {
            id: r.link_id,
            target: Some(EntityRef {
                id: r.target_id,
                structure_type: r.target_structure,
            }),
            source_property_id: r.source_property_id,
            created_at: Some(ts_from_millis(r.created_at)),
        })
        .collect();

    let referenced_dates = sqlx::query_scalar!(
        r#"SELECT iso_date AS "iso_date!" FROM referenced_dates
           WHERE entity_id = ? ORDER BY iso_date"#,
        id
    )
    .fetch_all(pool)
    .await?;

    let mut facets = sqlx::query_scalar!(
        r#"SELECT facet AS "facet!" FROM entity_facets WHERE entity_id = ? ORDER BY facet"#,
        id
    )
    .fetch_all(pool)
    .await?;
    sort_facets(&mut facets);

    Ok(Entity {
        id: row.id,
        structure_type: row.structure_type,
        name: row.name,
        properties,
        links,
        referenced_dates,
        created_at: Some(ts_from_millis(row.created_at)),
        updated_at: Some(ts_from_millis(row.updated_at)),
        facets,
    })
}

/// An `entities` row, typed so both filter branches of `load_entities` share it.
pub(super) struct EntityRow {
    id: String,
    structure_type: String,
    name: String,
    created_at: i64,
    updated_at: i64,
}

/// Hydrate every entity, or every entity of `structure_type`, most recently
/// updated first (ties by id), in five queries however many there are. Returns
/// what `load_entity` returns for each, field for field: both order links by
/// `link_id` and referenced dates by `iso_date` explicitly (I-48).
pub(crate) async fn load_entities(
    pool: &SqlitePool,
    structure_type: Option<&str>,
) -> Result<Vec<Entity>, AppError> {
    // Literal queries per filter rather than `? IS NULL OR …`, so each List query
    // gets a plan for its own filter. The order is explicit either way: most
    // recently updated first, ties broken by id (I-48).
    let rows = match structure_type {
        None => {
            sqlx::query_as!(
                EntityRow,
                r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                          created_at AS "created_at!", updated_at AS "updated_at!"
                   FROM entities ORDER BY updated_at DESC, id"#
            )
            .fetch_all(pool)
            .await?
        }
        Some(structure_type) => {
            sqlx::query_as!(
                EntityRow,
                r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                          created_at AS "created_at!", updated_at AS "updated_at!"
                   FROM entities WHERE structure_type = ? ORDER BY updated_at DESC, id"#,
                structure_type
            )
            .fetch_all(pool)
            .await?
        }
    };

    let mut entities: Vec<Entity> = rows.into_iter().map(entity_from_row).collect();
    let index: HashMap<String, usize> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i))
        .collect();

    // Each child query is restricted by the same filter. A row whose entity isn't
    // in `index` (created between the queries) is skipped.
    let prop_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", property_id AS "property_id!",
                  value_blob AS "value_blob!"
           FROM properties
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE ?1 IS NULL OR structure_type = ?1)
           ORDER BY entity_id, property_id"#,
        structure_type
    )
    .fetch_all(pool)
    .await?;
    for r in prop_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        let value: PropertyValue = prost::Message::decode(&*r.value_blob)?;
        entities[i].properties.insert(r.property_id, value);
    }

    let link_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", link_id AS "link_id!", target_id AS "target_id!",
                  target_structure AS "target_structure!",
                  source_property_id AS "source_property_id!", created_at AS "created_at!"
           FROM links
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE ?1 IS NULL OR structure_type = ?1)
           ORDER BY entity_id, link_id"#,
        structure_type
    )
    .fetch_all(pool)
    .await?;
    for r in link_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        entities[i].links.push(LinkRef {
            id: r.link_id,
            target: Some(EntityRef {
                id: r.target_id,
                structure_type: r.target_structure,
            }),
            source_property_id: r.source_property_id,
            created_at: Some(ts_from_millis(r.created_at)),
        });
    }

    let date_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", iso_date AS "iso_date!"
           FROM referenced_dates
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE ?1 IS NULL OR structure_type = ?1)
           ORDER BY entity_id, iso_date"#,
        structure_type
    )
    .fetch_all(pool)
    .await?;
    for r in date_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        entities[i].referenced_dates.push(r.iso_date);
    }

    let facet_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", facet AS "facet!"
           FROM entity_facets
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE ?1 IS NULL OR structure_type = ?1)
           ORDER BY entity_id, facet"#,
        structure_type
    )
    .fetch_all(pool)
    .await?;
    for r in facet_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        entities[i].facets.push(r.facet);
    }
    for entity in &mut entities {
        sort_facets(&mut entity.facets);
    }

    Ok(entities)
}

/// Hydrate the entities whose ids are in `ids`, as `load_entities` does, in five
/// queries. Ids with no entity are left out; the result is most recently
/// updated first, not in `ids` order.
pub(crate) async fn load_entities_by_id(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<Entity>, AppError> {
    let mut entities = load_in_order(pool, ids, &ReadMask::all()).await?;
    let updated = |e: &Entity| e.updated_at.map_or((0, 0), |t| (t.seconds, t.nanos));
    entities.sort_by(|a, b| updated(b).cmp(&updated(a)).then_with(|| a.id.cmp(&b.id)));
    Ok(entities)
}

/// Hydrate the entities whose ids are in `ids`, in that order, with the fields
/// `mask` asks for: up to five queries, each a lookup by id, and none for a
/// field left out. An id with no entity is left out; one given twice comes back
/// once. Unmasked, each entity is what `load_entity` returns.
pub(crate) async fn load_in_order(
    pool: &SqlitePool,
    ids: &[String],
    mask: &ReadMask,
) -> Result<Vec<Entity>, AppError> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let ids_json = serde_json::to_string(ids).map_err(|e| AppError::Invalid(e.to_string()))?;
    let rows = sqlx::query_as!(
        EntityRow,
        r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                  created_at AS "created_at!", updated_at AS "updated_at!"
           FROM entities WHERE id IN (SELECT value FROM json_each(?1))"#,
        ids_json
    )
    .fetch_all(pool)
    .await?;
    let mut by_id: HashMap<String, Entity> = rows
        .into_iter()
        .map(|row| (row.id.clone(), entity_from_row(row)))
        .collect();

    if !mask.no_properties() {
        let only = mask
            .property_ids()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        let prop_rows = sqlx::query!(
            r#"SELECT entity_id AS "entity_id!", property_id AS "property_id!",
                      value_blob AS "value_blob!"
               FROM properties
               WHERE entity_id IN (SELECT value FROM json_each(?1))
                 AND (?2 IS NULL OR property_id IN (SELECT value FROM json_each(?2)))"#,
            ids_json,
            only
        )
        .fetch_all(pool)
        .await?;
        for r in prop_rows {
            if let Some(entity) = by_id.get_mut(&r.entity_id) {
                let value: PropertyValue = prost::Message::decode(&*r.value_blob)?;
                entity.properties.insert(r.property_id, value);
            }
        }
    }

    if mask.links {
        let link_rows = sqlx::query!(
            r#"SELECT entity_id AS "entity_id!", link_id AS "link_id!", target_id AS "target_id!",
                      target_structure AS "target_structure!",
                      source_property_id AS "source_property_id!", created_at AS "created_at!"
               FROM links WHERE entity_id IN (SELECT value FROM json_each(?1))
               ORDER BY entity_id, link_id"#,
            ids_json
        )
        .fetch_all(pool)
        .await?;
        for r in link_rows {
            if let Some(entity) = by_id.get_mut(&r.entity_id) {
                entity.links.push(LinkRef {
                    id: r.link_id,
                    target: Some(EntityRef {
                        id: r.target_id,
                        structure_type: r.target_structure,
                    }),
                    source_property_id: r.source_property_id,
                    created_at: Some(ts_from_millis(r.created_at)),
                });
            }
        }
    }

    if mask.referenced_dates {
        let date_rows = sqlx::query!(
            r#"SELECT entity_id AS "entity_id!", iso_date AS "iso_date!"
               FROM referenced_dates WHERE entity_id IN (SELECT value FROM json_each(?1))
               ORDER BY entity_id, iso_date"#,
            ids_json
        )
        .fetch_all(pool)
        .await?;
        for r in date_rows {
            if let Some(entity) = by_id.get_mut(&r.entity_id) {
                entity.referenced_dates.push(r.iso_date);
            }
        }
    }

    if mask.facets {
        let facet_rows = sqlx::query!(
            r#"SELECT entity_id AS "entity_id!", facet AS "facet!"
               FROM entity_facets WHERE entity_id IN (SELECT value FROM json_each(?1))"#,
            ids_json
        )
        .fetch_all(pool)
        .await?;
        for r in facet_rows {
            if let Some(entity) = by_id.get_mut(&r.entity_id) {
                entity.facets.push(r.facet);
            }
        }
    }

    let mut entities = Vec::with_capacity(by_id.len());
    for id in ids {
        if let Some(mut entity) = by_id.remove(id) {
            sort_facets(&mut entity.facets);
            mask.apply(&mut entity);
            entities.push(entity);
        }
    }
    Ok(entities)
}

/// An entity with only its `entities` row read.
fn entity_from_row(row: EntityRow) -> Entity {
    Entity {
        id: row.id,
        structure_type: row.structure_type,
        name: row.name,
        properties: HashMap::new(),
        links: vec![],
        referenced_dates: vec![],
        created_at: Some(ts_from_millis(row.created_at)),
        updated_at: Some(ts_from_millis(row.updated_at)),
        facets: vec![],
    }
}

/// Convert unix-millis to a protobuf Timestamp.
pub(crate) fn ts_from_millis(millis: i64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: millis / 1000,
        nanos: ((millis % 1000) * 1_000_000) as i32,
    }
}
