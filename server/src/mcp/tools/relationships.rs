use futures::future::try_join_all;
use tonic::{Code, Request, Status};

use crate::structures::{self};
use calcifer::v1::entity_service_server::EntityService as EntityServiceTrait;
use calcifer::v1::{
    AddRelationshipRequest, Entity, ListBacklinksRequest, RemoveRelationshipRequest,
};

use super::{fail, is_code, timestamp_ms, Knowledge, Named, ToolError, ToolResult};

/// The link scope the server derives from an entity's direct relationships (ADR 12).
pub(super) const RELATED_SCOPE: &str = "@related";

pub(super) struct BacklinkSource {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) structure_type: String,
    pub(super) property_ids: Vec<String>,
}

impl Knowledge {
    /// The entities linking to `target_id`, newest link first. The server returns
    /// one Backlink per link row (self-links excluded), so a source that links
    /// from two properties is folded into one entry here.
    pub(super) async fn backlink_sources(
        &self,
        target_id: &str,
    ) -> Result<Vec<BacklinkSource>, ToolError> {
        let req = ListBacklinksRequest {
            entity_id: target_id.to_string(),
        };
        let res = self
            .entity
            .list_backlinks(Request::new(req))
            .await?
            .into_inner();
        let mut sources: Vec<BacklinkSource> = Vec::new();
        for backlink in res.backlinks {
            let Some(source) = backlink.source else {
                continue;
            };
            let at = if let Some(at) = sources.iter().position(|s| s.id == source.id) {
                at
            } else {
                sources.push(BacklinkSource {
                    id: source.id,
                    name: source.name,
                    structure_type: source.structure_type,
                    property_ids: Vec::new(),
                });
                sources.len() - 1
            };
            sources[at].property_ids.push(backlink.source_property_id);
        }
        Ok(sources)
    }

    /// How get_note lists the entities `entity` holds direct relationships to
    /// (its `@related` links): each by `shown_name`, so a name other notes share
    /// is its id. Oldest first, ties by target id as the server orders the rows
    /// it derives them from. A target that no longer exists is skipped.
    pub(super) async fn related_names(&self, entity: &Entity) -> Result<Vec<String>, Status> {
        let mut links: Vec<(i64, &str)> = entity
            .links
            .iter()
            .filter(|l| l.source_property_id == RELATED_SCOPE)
            .filter_map(|l| {
                let target = l.target.as_ref()?;
                let at = l.created_at.as_ref().map_or(0, timestamp_ms);
                Some((at, target.id.as_str()))
            })
            .collect();
        links.sort_unstable();
        let found = try_join_all(links.into_iter().map(|(_, id)| async move {
            match self.entity(id).await {
                Ok(target) => Ok(Some(self.shown_name(&target).await?)),
                Err(e) if is_code(&e, Code::NotFound) => Ok(None),
                Err(e) => Err(e),
            }
        }))
        .await?;
        Ok(found.into_iter().flatten().collect())
    }

    pub async fn get_backlinks(&self, name: &str) -> ToolResult {
        let note = self.existing_note(name).await?;
        let sources = self.backlink_sources(&note.id).await?;
        if sources.is_empty() {
            return Ok(format!("Nothing links to \"{}\" yet.", note.name));
        }
        Ok(sources
            .iter()
            .map(|s| format!("- {}", backlink_label(s)))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    /// The Note `value` names or has as its id, as `named_note` finds it (a name
    /// several notes share is refused). A value that names only something else
    /// (a Tag, a day) is told only Notes can be related.
    pub(super) async fn note_by_name_or_id(&self, value: &str) -> Result<Entity, ToolError> {
        let kind = match self.named_note(value).await? {
            Named::One(entity) if entity.structure_type == "Note" => return Ok(*entity),
            Named::One(entity) => entity.structure_type,
            Named::Other(kind) => kind,
            Named::Nothing => {
                return fail(format!(
                    "No note named or with id \"{}\". Use search_notes.",
                    value.trim()
                ))
            }
        };
        fail(format!(
            "\"{}\" is a {kind}, not a Note; only Notes can be related.",
            value.trim()
        ))
    }

    /// Both ends of link_notes / unlink_notes, `from` resolved first so its error
    /// is the one reported.
    pub(super) async fn relationship_ends(
        &self,
        from: &str,
        to: &str,
    ) -> Result<(Entity, Entity), ToolError> {
        let from = self.note_by_name_or_id(from).await?;
        let to = self.note_by_name_or_id(to).await?;
        Ok((from, to))
    }

    /// Relate one note to another (a direct relationship, ADR 12): no text is
    /// written, and relating twice is a no-op that the result reports.
    pub async fn link_notes(&self, from: &str, to: &str) -> ToolResult {
        let (from, to) = self.relationship_ends(from, to).await?;
        let was_related = is_related(&from, &to.id);
        let req = AddRelationshipRequest {
            entity_id: from.id.clone(),
            target_id: to.id.clone(),
        };
        self.entity.add_relationship(Request::new(req)).await?;
        Ok(if was_related {
            format!("\"{}\" was already related to \"{}\".", from.name, to.name)
        } else {
            format!("Related \"{}\" -> \"{}\".", from.name, to.name)
        })
    }

    /// The mirror of link_notes: remove the direct relationship, if there is one.
    pub async fn unlink_notes(&self, from: &str, to: &str) -> ToolResult {
        let (from, to) = self.relationship_ends(from, to).await?;
        let was_related = is_related(&from, &to.id);
        let req = RemoveRelationshipRequest {
            entity_id: from.id.clone(),
            target_id: to.id.clone(),
        };
        self.entity.remove_relationship(Request::new(req)).await?;
        Ok(if was_related {
            format!("Unrelated \"{}\" -> \"{}\".", from.name, to.name)
        } else {
            format!("\"{}\" wasn't related to \"{}\".", from.name, to.name)
        })
    }
}

/// Whether `entity` holds a direct relationship to `target_id`.
pub(super) fn is_related(entity: &Entity, target_id: &str) -> bool {
    entity.links.iter().any(|l| {
        l.source_property_id == RELATED_SCOPE
            && l.target.as_ref().is_some_and(|t| t.id == target_id)
    })
}

/// The source's name and id, then why it links: `related` for a direct
/// relationship, then `via` the properties that aren't rich-text docs. A source
/// that links only from its docs has no reason.
pub(super) fn backlink_label(source: &BacklinkSource) -> String {
    let docs = structures::richtext_properties(&source.structure_type);
    let mut reasons: Vec<String> = Vec::new();
    if source.property_ids.iter().any(|p| p == RELATED_SCOPE) {
        reasons.push("related".into());
    }
    let via: Vec<&str> = source
        .property_ids
        .iter()
        .map(String::as_str)
        .filter(|p| *p != RELATED_SCOPE && !docs.contains(p))
        .collect();
    if !via.is_empty() {
        reasons.push(format!("via {}", via.join(", ")));
    }
    let named = format!("{} ({})", source.name, source.id);
    if reasons.is_empty() {
        named
    } else {
        format!("{named}: {}", reasons.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backlink_label_names_relationships_and_non_doc_properties() {
        let label = |property_ids: &[&str]| {
            backlink_label(&BacklinkSource {
                id: "a1".into(),
                name: "A".into(),
                structure_type: "Note".into(),
                property_ids: property_ids
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
            })
        };
        assert_eq!(
            label(&["content"]),
            "A (a1)",
            "rich-text sources give no reason"
        );
        assert_eq!(label(&["@related"]), "A (a1): related");
        assert_eq!(label(&["content", "@related"]), "A (a1): related");
        assert_eq!(label(&["tags"]), "A (a1): via tags");
        assert_eq!(label(&["content", "tags"]), "A (a1): via tags");
        assert_eq!(
            label(&["tags", "content", "@related"]),
            "A (a1): related, via tags",
            "related comes first"
        );
    }
}
