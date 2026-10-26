use std::collections::HashMap;

use futures::future::try_join_all;
use serde_json::json;
use tonic::{Code, Request, Status};

use crate::mcp::markdown::{self, Reference};
use crate::services::entity::check_name_chars;
use crate::structures::{self};
use calcifer::v1::entity_service_server::EntityService as EntityServiceTrait;
use calcifer::v1::search_service_server::SearchService as SearchServiceTrait;
use calcifer::v1::{
    property_value, AddRelationTargetRequest, CreateEntityRequest, Entity, EntityRef,
    EntityRefList, MatchRange, PropertyValue, RemoveRelationTargetRequest, SearchMode,
    SearchRequest,
};

use super::doc::{doc_ref, unknown_targets};
use super::edit::{frontmatter_tags, strip_frontmatter};
use super::{fail, is_code, timestamp_ms, Knowledge, NoteKey, ToolError, ToolResult};

impl Knowledge {
    /// Create a note that doesn't exist yet. An existing note is an error, not an
    /// overwrite: edit_note and append_to_note are how a note's body changes.
    /// Links are checked before the note is created, so a failed call leaves
    /// nothing behind; the note's links to itself are the one target allowed to
    /// be missing. Each of `tags` is resolved, or created, as a Tag and set as
    /// the note's `tags` property, in the transaction that creates the note
    /// (I-73); a body #tag only references its Tag. With no
    /// `tags`, the `tags:` line of the markdown's leading frontmatter is used, so
    /// a get_note result keeps its tags when copied into a new note; `Some`, even
    /// an empty list, wins over the frontmatter.
    pub async fn create_note(
        &self,
        name: &str,
        markdown: &str,
        tags: Option<&[String]>,
    ) -> ToolResult {
        let exists = || {
            ToolError(format!(
                "A note named \"{name}\" already exists; nothing was written. Use get_note, then edit_note or append_to_note to change it."
            ))
        };
        match self.resolve_name("Note", name, false).await {
            Ok(_) => return Err(exists()),
            Err(e) if is_code(&e, Code::NotFound) => {}
            Err(e) => return Err(e.into()),
        }
        // Before anything is written: resolving #tags below creates Tags, and a
        // refused name would leave them behind.
        check_name_chars(name.trim())?;

        // Before the first write: resolving the body's #tags creates Tags.
        let tags = match tags {
            Some(tags) => tags.to_vec(),
            None => frontmatter_tags(markdown)?.unwrap_or_default(),
        };
        let parsed = markdown::parse(strip_frontmatter(markdown));
        let (mut ids, missing) = self.resolve_references(&parsed.references).await?;
        let own_key = Reference {
            structure_type: "Note".into(),
            name: name.into(),
        }
        .key();
        let missing: Vec<&Reference> = missing.into_iter().filter(|r| r.key() != own_key).collect();
        if !missing.is_empty() {
            return Err(unknown_targets(&missing));
        }
        let tag_ids = self.resolve_tags(&tags).await?;

        // Note names aren't unique, so a create_note of the same name that also
        // got past the check above makes a second note rather than an error;
        // resolving the name returns the older one.
        let mut properties = HashMap::new();
        if !tag_ids.is_empty() {
            properties.insert("tags".to_string(), tags_value(tag_ids));
        }
        let req = CreateEntityRequest {
            structure_type: "Note".into(),
            name: Some(name.to_string()),
            properties,
            facets: vec![],
        };
        let created = self.entity.create_entity(Request::new(req)).await?;
        let id = created
            .into_inner()
            .entity
            .map(|e| e.id)
            .unwrap_or_default();
        ids.insert(own_key, id.clone());
        // Unconditional: the note was created just above, so it has no body to lose.
        let body = doc_ref("Note", &id)?;
        self.put_doc(&body, &parsed.resolve(&ids), None).await?;
        let ent = self.entity(&id).await?;
        let wikilinks: Vec<_> = ent
            .links
            .iter()
            .filter(|l| l.source_property_id == body.property_id)
            .collect();
        let linked = wikilinks
            .iter()
            .map(|l| l.target.as_ref().map_or("", |t| t.structure_type.as_str()))
            .collect::<Vec<_>>();
        let linked = if linked.is_empty() {
            "none".to_string()
        } else {
            linked.join(", ")
        };
        Ok(format!(
            "Saved note \"{}\" ({id}). Wikilinks: {} [{linked}]; dates: {}; tags: {}.",
            ent.name,
            wikilinks.len(),
            ent.referenced_dates.len(),
            ent.links.len() - wikilinks.len()
        ))
    }

    /// Each tag name's Tag id, created when missing, without repeats.
    pub(super) async fn resolve_tags(&self, tags: &[String]) -> Result<Vec<String>, ToolError> {
        let found = try_join_all(tags.iter().map(|t| self.resolve_name("Tag", t, true))).await?;
        let mut ids: Vec<String> = Vec::new();
        for id in found.into_iter().filter_map(|res| res.entity.map(|e| e.id)) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub async fn append_to_note(&self, name: &str, markdown: &str) -> ToolResult {
        let id = self.existing_note_id(name).await?;
        let addition = self.parse_markdown(markdown).await?;
        self.append_doc(
            &doc_ref("Note", &id)?,
            &format!("Note \"{name}\""),
            &addition,
        )
        .await?;
        let ent = self.entity(&id).await?;
        Ok(format!(
            "Appended to \"{}\" ({id}); now {} wikilink(s).",
            ent.name,
            ent.links.len()
        ))
    }

    /// Tag or untag an existing note, one tag at a time through the entity
    /// service, so a tag added by someone else meanwhile is kept. Each `add` name
    /// is resolved, or created, as a Tag; a `remove` name that is no Tag, or that
    /// the note doesn't have, is skipped. Nothing is written when both lists are
    /// empty or a name is in both. Returns the note's tags afterwards.
    pub async fn tag_entity(&self, name: &str, add: &[String], remove: &[String]) -> ToolResult {
        let id = self.existing_note_id(name).await?;
        if add.is_empty() && remove.is_empty() {
            return fail("Give at least one tag name in add or remove; nothing was written.");
        }
        let removed: Vec<String> = remove.iter().map(|t| structures::fold_name(t)).collect();
        if let Some(both) = add
            .iter()
            .find(|t| removed.contains(&structures::fold_name(t)))
        {
            return fail(format!(
                "\"{both}\" is in both add and remove; nothing was written."
            ));
        }

        let add_ids = self.resolve_tags(add).await?;
        let mut remove_ids: Vec<String> = Vec::new();
        for tag in remove {
            match self.resolve_name("Tag", tag, false).await {
                Ok(res) => {
                    let tag_id = res.entity.map(|e| e.id).unwrap_or_default();
                    if !remove_ids.contains(&tag_id) {
                        remove_ids.push(tag_id);
                    }
                }
                Err(e) if is_code(&e, Code::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }

        for tag_id in add_ids {
            let req = AddRelationTargetRequest {
                entity_id: id.clone(),
                property_id: "tags".into(),
                target_id: tag_id,
            };
            self.entity.add_relation_target(Request::new(req)).await?;
        }
        for tag_id in remove_ids {
            let req = RemoveRelationTargetRequest {
                entity_id: id.clone(),
                property_id: "tags".into(),
                target_id: tag_id,
            };
            self.entity
                .remove_relation_target(Request::new(req))
                .await?;
        }

        let ent = self.entity(&id).await?;
        let names = self.tag_names(&ent).await?;
        let tags = if names.is_empty() {
            "none".to_string()
        } else {
            names.join(", ")
        };
        Ok(format!("Tags on \"{}\": {tags}.", ent.name))
    }

    /// The names of the Tags in the entity's `tags` property, in stored order. A
    /// ref whose Tag no longer exists is skipped.
    pub(super) async fn tag_names(&self, entity: &Entity) -> Result<Vec<String>, Status> {
        let Some(property_value::Value::Relations(list)) =
            entity.properties.get("tags").and_then(|v| v.value.as_ref())
        else {
            return Ok(Vec::new());
        };
        let mut names = Vec::new();
        for tag in &list.refs {
            match self.entity(&tag.id).await {
                Ok(tag) => names.push(tag.name),
                Err(e) if is_code(&e, Code::NotFound) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(names)
    }

    /// The note as markdown under a frontmatter block: name, version, tags and
    /// related for a note, name, version and date for a daily note (it has no tags
    /// or relationships). Values are JSON, which is valid YAML, so a name with a
    /// colon or a quote can't break the block. `version` is what edit_note checks
    /// its line numbers against; `tags` is the names in the note's `tags`
    /// property, `[]` when it has none, and what create_note reads back; `related`
    /// is the names of the notes it holds direct relationships to, oldest first,
    /// `[]` when none, and create_note ignores it; `date` is the day's ISO date.
    /// A note or a day that isn't there is an error: nothing is created.
    pub async fn get_note(&self, key: NoteKey<'_>) -> ToolResult {
        let found = self.existing_doc(&key).await?;
        let (doc, updated_at) = self
            .get_doc(&doc_ref(&found.structure_type, &found.id)?)
            .await?;
        let (body, _) = self.render_body(&doc).await?;
        let version = timestamp_ms(&updated_at);
        let frontmatter = if found.structure_type == "DailyNote" {
            format!(
                "name: {}\nversion: {version}\ndate: {}",
                json!(found.name),
                json!(found.date.as_deref().unwrap_or_default())
            )
        } else {
            let tags = self.tag_names(&found.entity).await?;
            let related = self.related_names(&found.entity).await?;
            format!(
                "name: {}\nversion: {version}\ntags: {}\nrelated: {}",
                json!(found.name),
                json!(tags),
                json!(related)
            )
        };
        Ok(format!("---\n{frontmatter}\n---\n{body}"))
    }

    pub async fn search_notes(&self, query: &str, limit: u32, mode: SearchMode) -> ToolResult {
        // The server falls back to lexical for semantic/hybrid when embeddings are off.
        let req = SearchRequest {
            query: query.to_string(),
            limit,
            mode: mode as i32,
        };
        let res = self.search.search(Request::new(req)).await?.into_inner();
        if res.hits.is_empty() {
            return Ok(format!("No matches for \"{query}\"."));
        }
        let lines: Vec<String> = res
            .hits
            .iter()
            .map(|h| {
                let detail = if h.snippet.is_empty() {
                    format!("(score {:.3})", h.score)
                } else {
                    mark_matches(&h.snippet, &h.matches)
                };
                let entity = h.entity.clone().unwrap_or_default();
                format!("- [{}] {}: {detail}", entity.structure_type, entity.name)
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

/// Bold each matched range of a plain-text snippet. `**` rather than brackets,
/// so a match never reads as [[wikilink]] syntax. Ranges are UTF-16 offsets;
/// they're taken in order, and where they overlap or run past the snippet only
/// the part not yet bolded counts.
pub(super) fn mark_matches(snippet: &str, matches: &[MatchRange]) -> String {
    let units: Vec<u16> = snippet.encode_utf16().collect();
    let slice = |from: usize, to: usize| String::from_utf16_lossy(&units[from..to]);
    let mut ranges: Vec<(usize, usize)> = matches
        .iter()
        .map(|m| (m.start as usize, m.end as usize))
        .collect();
    ranges.sort_unstable();
    let mut out = String::new();
    let mut at = 0;
    for (start, end) in ranges {
        let start = start.clamp(at, units.len());
        let end = end.clamp(start, units.len());
        if start == end {
            continue;
        }
        out += &format!("{}**{}**", slice(at, start), slice(start, end));
        at = end;
    }
    out + &slice(at, units.len())
}

/// A `tags` value naming these Tags.
fn tags_value(tag_ids: Vec<String>) -> PropertyValue {
    let refs = tag_ids
        .into_iter()
        .map(|id| EntityRef {
            id,
            structure_type: "Tag".into(),
        })
        .collect();
    PropertyValue {
        value: Some(property_value::Value::Relations(EntityRefList { refs })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_bolded_by_utf16_offset() {
        let ranges = [
            MatchRange { start: 3, end: 5 },
            MatchRange { start: 9, end: 10 },
        ];
        // "😀" is two UTF-16 units, so "ab" starts at 3.
        assert_eq!(mark_matches("😀 abc d", &ranges[..1]), "😀 **ab**c d");
        assert_eq!(mark_matches("no matches", &[]), "no matches");
        assert_eq!(mark_matches("0123456789", &ranges), "012**34**5678**9**");
    }

    #[test]
    fn overlapping_unordered_and_stray_matches_dont_panic() {
        let range = |start, end| MatchRange { start, end };
        let messy = [
            range(6, 8),
            range(1, 4),
            range(2, 5),
            range(3, 3),
            range(9, 99),
            range(50, 60),
        ];
        assert_eq!(
            mark_matches("0123456789", &messy),
            "0**123****4**5**67**8**9**"
        );
        assert_eq!(
            mark_matches("abc", &[range(2, 1)]),
            "abc",
            "an end before its start is empty"
        );
    }
}
