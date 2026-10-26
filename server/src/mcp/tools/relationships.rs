use futures::future::try_join_all;
use tonic::{Code, Request, Status};

use crate::structures::{self};
use calcifer::v1::entity_service_server::EntityService as EntityServiceTrait;
use calcifer::v1::{
    AddRelationshipRequest, Entity, ListBacklinksRequest, ListEntitiesRequest,
    RemoveRelationshipRequest,
};

use super::{fail, is_code, timestamp_ms, Knowledge, ToolError, ToolResult};

/// The link scope the server derives from an entity's direct relationships (ADR 12).
pub(super) const RELATED_SCOPE: &str = "@related";

pub(super) struct BacklinkSource {
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
        let mut sources: Vec<(String, BacklinkSource)> = Vec::new();
        for backlink in res.backlinks {
            let Some(source) = backlink.source else {
                continue;
            };
            let at = if let Some(at) = sources.iter().position(|(id, _)| *id == source.id) {
                at
            } else {
                let entry = BacklinkSource {
                    name: source.name,
                    structure_type: source.structure_type,
                    property_ids: Vec::new(),
                };
                sources.push((source.id, entry));
                sources.len() - 1
            };
            sources[at].1.property_ids.push(backlink.source_property_id);
        }
        Ok(sources.into_iter().map(|(_, s)| s).collect())
    }

    /// The names of the entities `entity` holds direct relationships to (its
    /// `@related` links), oldest first, ties by target id as the server orders the
    /// rows it derives them from. A target that no longer exists is skipped.
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
                Ok(target) => Ok(Some(target.name)),
                Err(e) if is_code(&e, Code::NotFound) => Ok(None),
                Err(e) => Err(e),
            }
        }))
        .await?;
        Ok(found.into_iter().flatten().collect())
    }

    pub async fn get_backlinks(&self, name: &str) -> ToolResult {
        let id = self.existing_note_id(name).await?;
        let sources = self.backlink_sources(&id).await?;
        if sources.is_empty() {
            return Ok(format!("Nothing links to \"{name}\" yet."));
        }
        Ok(sources
            .iter()
            .map(|s| format!("- {}", backlink_label(s)))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    /// The Note `value` names: an id when it parses as a UUID and names an
    /// existing Note, else a name matched case-insensitively over every Note.
    /// Nothing is created. Notes have no name key, so several matches is an error
    /// that lists them for the agent to call again by id.
    pub(super) async fn note_by_name_or_id(&self, value: &str) -> Result<Entity, ToolError> {
        let value = value.trim();
        if uuid::Uuid::parse_str(value).is_ok() {
            match self.entity(value).await {
                Ok(entity) if entity.structure_type == "Note" => return Ok(entity),
                Ok(_) => {}
                Err(e) if is_code(&e, Code::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let req = ListEntitiesRequest {
            structure_type: "Note".into(),
        };
        let notes = self
            .entity
            .list_entities(Request::new(req))
            .await?
            .into_inner()
            .entities;
        let folded = structures::fold_name(value);
        let mut matches: Vec<Entity> = notes
            .into_iter()
            .filter(|n| structures::fold_name(&n.name) == folded)
            .collect();
        match matches.len() {
            0 => fail(format!(
                "No note named or with id \"{value}\". Use search_notes."
            )),
            1 => Ok(matches.remove(0)),
            _ => {
                let listed: Vec<String> = matches
                    .iter()
                    .map(|n| format!("- {} ({})", n.name, n.id))
                    .collect();
                fail(format!(
                    "Several notes are named \"{value}\":\n{}\nCall again with the id.",
                    listed.join("\n")
                ))
            }
        }
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

/// The source's name, plus why it links: `related` for a direct relationship,
/// then `via` the properties that aren't rich-text docs. A source that links only
/// from its docs stays bare.
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
    if reasons.is_empty() {
        source.name.clone()
    } else {
        format!("{} ({})", source.name, reasons.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backlink_label_names_relationships_and_non_doc_properties() {
        let label = |property_ids: &[&str]| {
            backlink_label(&BacklinkSource {
                name: "A".into(),
                structure_type: "Note".into(),
                property_ids: property_ids
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
            })
        };
        assert_eq!(label(&["content"]), "A", "rich-text sources stay bare");
        assert_eq!(label(&["@related"]), "A (related)");
        assert_eq!(label(&["content", "@related"]), "A (related)");
        assert_eq!(label(&["tags"]), "A (via tags)");
        assert_eq!(label(&["content", "tags"]), "A (via tags)");
        assert_eq!(
            label(&["tags", "content", "@related"]),
            "A (related, via tags)",
            "related comes first"
        );
    }
}
