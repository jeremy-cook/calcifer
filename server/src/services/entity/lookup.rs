use tonic::Status;

use crate::error::AppError;
use crate::structures::{self, FacetDef};

/// Put facet ids in registry order (`structures::FACETS`). An id missing from
/// the registry is kept, after the known ones, in id order.
pub(super) fn sort_facets(facets: &mut [String]) {
    facets.sort_by_cached_key(|f| {
        let rank = structures::FACETS
            .iter()
            .position(|d| d.id == f)
            .unwrap_or(usize::MAX);
        (rank, f.clone())
    });
}

/// The registry entry for `structure_type`, or `InvalidArgument` for a type the
/// registry doesn't declare (I-24). Every write or lookup that takes a
/// `structure_type` goes through this, so an unknown type can't create an entity.
pub(super) fn known_structure(
    structure_type: &str,
) -> Result<&'static structures::StructureDef, Status> {
    structures::structure(structure_type).ok_or_else(|| {
        Status::invalid_argument(format!("unknown structure_type {structure_type:?}"))
    })
}

/// The registry entry for facet `id`, or `InvalidArgument` for one the registry
/// doesn't declare.
pub(super) fn known_facet(id: &str) -> Result<&'static FacetDef, Status> {
    structures::facet(id).ok_or_else(|| Status::invalid_argument(format!("unknown facet {id:?}")))
}

/// `known_facet`, and `FailedPrecondition` unless `structure_type` accepts it
/// (`StructureDef.facets`). Create and AttachFacet check a facet with this.
pub(super) fn accepted_facet(structure_type: &str, id: &str) -> Result<&'static FacetDef, Status> {
    let facet = known_facet(id)?;
    let accepts =
        structures::structure(structure_type).is_some_and(|s| s.facets.contains(&facet.id));
    if !accepts {
        return Err(Status::failed_precondition(format!(
            "a {structure_type} doesn't accept the {} facet",
            facet.id
        )));
    }
    Ok(facet)
}

/// The facets attached to `entity_id`, read inside a write transaction.
pub(super) async fn attached_facets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
) -> Result<Vec<String>, AppError> {
    let facets = sqlx::query_scalar!(
        r#"SELECT facet AS "facet!" FROM entity_facets WHERE entity_id = ?"#,
        entity_id
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(facets)
}

/// An entity's `structure_type` and attached facets, read inside a write
/// transaction, or `NotFound`.
pub(super) async fn structure_and_facets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
) -> Result<(String, Vec<String>), Status> {
    let structure_type = sqlx::query_scalar!(
        "SELECT structure_type FROM entities WHERE id = ?",
        entity_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| Status::not_found(format!("entity {entity_id}")))?;
    let facets = attached_facets(tx, entity_id).await?;
    Ok((structure_type, facets))
}
