//! Writes to the `links` table: one scoped replace that both link sources share,
//! and the existence / real-type lookup for mention targets.

use std::collections::BTreeSet;

use crate::error::AppError;
use crate::tiptap::references::MentionRef;

/// A link target that exists, with its real `structure_type` (I-51).
#[derive(Debug, Clone)]
pub(crate) struct LinkTarget {
    pub id: String,
    pub structure_type: String,
}

/// Look up each mention's real `structure_type`, dropping targets with no
/// `entities` row. Safe because the server is authoritative — unlike the FE, where
/// such a chip is a tombstone, not a deletion. Links record the target's real type,
/// never a type the caller claims (I-51).
#[tracing::instrument(skip_all)]
pub(crate) async fn live_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    refs: &[MentionRef],
) -> Result<Vec<LinkTarget>, AppError> {
    let mut targets = Vec::with_capacity(refs.len());
    for m in refs {
        // Runtime (non-macro) query: same vec0-virtual-table compile-time
        // introspection hazard documented at fts_upsert_name forces unchecked here.
        let structure_type =
            sqlx::query_scalar::<_, String>("SELECT structure_type FROM entities WHERE id = ?")
                .bind(&m.id)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(structure_type) = structure_type {
            targets.push(LinkTarget {
                id: m.id.clone(),
                structure_type,
            });
        }
    }
    Ok(targets)
}

/// Scoped link replace for a single (entity_id, source_property_id): select existing
/// link_id/created_at pairs for that scope, delete them, then re-insert one row per
/// target, reusing the prior link_id/created_at when a target repeats (preserves
/// backlink recency across an edit). Targets come from `live_targets` or
/// `relations::check_relation_targets`, so they exist and carry their real type.
/// Runs inside an already-open transaction. Shared by `RichTextService.PutRichText`
/// (content-derived links) and `relations::sync_relation_property` (relation-property links).
/// Returns the ids of the targets the scope gained or lost, whose backlinks
/// changed, so the writer publishes them (ADR 9).
#[tracing::instrument(skip_all)]
pub(crate) async fn replace_scoped_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    source_property_id: &str,
    targets: &[LinkTarget],
    now: i64,
) -> Result<BTreeSet<String>, AppError> {
    let existing = sqlx::query!(
        "SELECT link_id, target_id, created_at FROM links
         WHERE entity_id = ? AND source_property_id = ?",
        entity_id,
        source_property_id
    )
    .fetch_all(&mut **tx)
    .await?;
    let prev: std::collections::HashMap<String, (String, i64)> = existing
        .into_iter()
        .map(|row| (row.target_id, (row.link_id, row.created_at)))
        .collect();

    sqlx::query!(
        "DELETE FROM links WHERE entity_id = ? AND source_property_id = ?",
        entity_id,
        source_property_id
    )
    .execute(&mut **tx)
    .await?;

    let kept: BTreeSet<&str> = targets.iter().map(|t| t.id.as_str()).collect();
    let mut touched: BTreeSet<String> = prev
        .keys()
        .filter(|id| !kept.contains(id.as_str()))
        .cloned()
        .collect();
    touched.extend(
        kept.iter()
            .filter(|id| !prev.contains_key(**id))
            .map(|id| (*id).to_string()),
    );

    for m in targets {
        let (link_id, created_at) = match prev.get(&m.id) {
            Some((lid, ts)) => (lid.clone(), *ts),
            None => (uuid::Uuid::new_v4().to_string(), now),
        };
        sqlx::query!(
            "INSERT INTO links (entity_id, link_id, target_id, target_structure, source_property_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            entity_id,
            link_id,
            m.id,
            m.structure_type,
            source_property_id,
            created_at,
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(touched)
}
