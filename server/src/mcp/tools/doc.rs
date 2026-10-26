use std::collections::HashMap;

use futures::future::try_join_all;
use serde_json::{json, Value};
use tonic::{Code, Request, Status};

use crate::mcp::markdown::{self, ChipTarget, Reference};
use crate::services::entity::ts_from_millis;
use crate::structures::{self};
use calcifer::v1::entity_service_server::EntityService as EntityServiceTrait;
use calcifer::v1::rich_text_service_server::RichTextService as RichTextServiceTrait;
use calcifer::v1::{
    resolve_entity_request, GetRichTextRequest, PutRichTextRequest, ResolveEntityRequest,
    ResolveEntityResponse,
};

use super::lines::BlockSpan;
use super::{blocks, empty_doc, fail, is_code, Knowledge, ToolError};

/// What a write that links to missing targets tells the agent: each target as
/// it would be written, and how to fix the call.
pub(super) fn unknown_targets(missing: &[&Reference]) -> ToolError {
    let listed: Vec<String> = missing
        .iter()
        .map(|r| {
            markdown::mention_markdown(&ChipTarget {
                name: r.name.clone(),
                structure_type: r.structure_type.clone(),
            })
        })
        .collect();
    ToolError(format!(
        "Nothing was written: {} link(s) to something that doesn't exist: {}. Fix the name (search_notes finds existing ones) or create the target first: create_note for a note, create_daily_note for a day.",
        listed.len(),
        listed.join(", ")
    ))
}

/// The body doc of an entity: its structure's first declared rich-text property.
pub(in crate::mcp) struct DocRef {
    pub(super) entity_id: String,
    pub(super) property_id: String,
}

pub(in crate::mcp) fn doc_ref(structure_type: &str, entity_id: &str) -> Result<DocRef, ToolError> {
    match structures::richtext_properties(structure_type).first() {
        Some(property_id) => Ok(DocRef { entity_id: entity_id.to_string(), property_id: property_id.to_string() }),
        None => fail(format!(
            "Structure \"{structure_type}\" declares no rich-text property, so it has no body to read or write."
        )),
    }
}

pub(super) const APPEND_ATTEMPTS: usize = 3;

impl Knowledge {
    /// Get (or, with `create_if_missing`, create) the (structure_type, name)
    /// entity. NotFound when it's missing and `create_if_missing` is false.
    pub(super) async fn resolve_name(
        &self,
        structure_type: &str,
        name: &str,
        create_if_missing: bool,
    ) -> Result<ResolveEntityResponse, Status> {
        let req = ResolveEntityRequest {
            structure_type: structure_type.to_string(),
            key: Some(resolve_entity_request::Key::Name(name.to_string())),
            create_if_missing,
        };
        Ok(self
            .entity
            .resolve_entity(Request::new(req))
            .await?
            .into_inner())
    }

    /// [[wikilinks]] / #tags resolve to canonical entities, keyed by
    /// `Reference::key`, plus the references that name nothing. Only a #tag is
    /// created when missing: it has no body to write and no tool creates one,
    /// while a [[wikilink]] to a missing target is a mistake to report, not a
    /// note to invent.
    pub(super) async fn resolve_references<'a>(
        &self,
        references: &'a [Reference],
    ) -> Result<(HashMap<String, String>, Vec<&'a Reference>), ToolError> {
        let found = try_join_all(references.iter().map(|r| async move {
            let create = r.structure_type == "Tag";
            match self.resolve_name(&r.structure_type, &r.name, create).await {
                Ok(res) => Ok((r, res.entity.map(|e| e.id))),
                Err(e) if is_code(&e, Code::NotFound) => Ok((r, None)),
                Err(e) => Err(e),
            }
        }))
        .await?;
        let mut ids = HashMap::new();
        let mut missing = Vec::new();
        for (r, id) in found {
            match id {
                Some(id) => {
                    ids.insert(r.key(), id);
                }
                None => missing.push(r),
            }
        }
        Ok((ids, missing))
    }

    /// The markdown as a doc with every chip resolved. A link to anything that
    /// doesn't exist fails the whole write, so nothing is saved half-linked.
    pub(super) async fn parse_markdown(&self, markdown: &str) -> Result<Value, ToolError> {
        let parsed = markdown::parse(markdown);
        let (ids, missing) = self.resolve_references(&parsed.references).await?;
        if !missing.is_empty() {
            return Err(unknown_targets(&missing));
        }
        Ok(parsed.resolve(&ids))
    }

    /// Each chip target's current name and structure, fetched in parallel. A
    /// target that no longer exists is left out.
    pub(super) async fn chip_targets(
        &self,
        doc: &Value,
    ) -> Result<HashMap<String, ChipTarget>, ToolError> {
        let found = try_join_all(markdown::chip_ids(doc).into_iter().map(|id| async move {
            match self.entity(&id).await {
                Ok(e) => Ok(Some((
                    id,
                    ChipTarget {
                        name: e.name,
                        structure_type: e.structure_type,
                    },
                ))),
                Err(e) if is_code(&e, Code::NotFound) => Ok(None),
                Err(e) => Err(e),
            }
        }))
        .await?;
        Ok(found.into_iter().flatten().collect())
    }

    /// The note body as markdown (blocks separated by a blank line), plus the
    /// span of each block so a line range can be mapped back onto blocks.
    pub(super) async fn render_body(
        &self,
        doc: &Value,
    ) -> Result<(String, Vec<BlockSpan>), ToolError> {
        let targets = self.chip_targets(doc).await?;
        let mut parts = Vec::new();
        let mut spans = Vec::new();
        let mut line = 1;
        for (index, block) in blocks(doc).iter().enumerate() {
            let md = markdown::block_markdown(block, &targets);
            if md.is_empty() {
                continue;
            }
            let last = line + md.split('\n').count() - 1;
            spans.push(BlockSpan {
                index,
                first: line,
                last,
            });
            parts.push(md);
            line = last + 2;
        }
        Ok((parts.join("\n\n"), spans))
    }

    /// A declared-but-unsaved doc comes back empty at the epoch (see
    /// RichTextService.GetRichText), so `updated_at` is always something a
    /// conditional PutRichText can echo back.
    pub(in crate::mcp) async fn get_doc(
        &self,
        r: &DocRef,
    ) -> Result<(Value, prost_types::Timestamp), ToolError> {
        let req = GetRichTextRequest {
            entity_id: r.entity_id.clone(),
            property_id: r.property_id.clone(),
        };
        let rt = self
            .richtext
            .get_rich_text(Request::new(req))
            .await?
            .into_inner()
            .rich_text
            .unwrap_or_default();
        let doc = if rt.doc.is_empty() {
            empty_doc()
        } else {
            serde_json::from_str(&rt.doc)
                .map_err(|e| ToolError(format!("the stored document isn't valid JSON: {e}")))?
        };
        Ok((doc, rt.updated_at.unwrap_or_else(|| ts_from_millis(0))))
    }

    pub(in crate::mcp) async fn put_doc(
        &self,
        r: &DocRef,
        doc: &Value,
        expected_updated_at: Option<prost_types::Timestamp>,
    ) -> Result<(), Status> {
        let req = PutRichTextRequest {
            entity_id: r.entity_id.clone(),
            property_id: r.property_id.clone(),
            doc: doc.to_string(),
            expected_updated_at,
        };
        self.richtext.put_rich_text(Request::new(req)).await?;
        Ok(())
    }

    /// Read, merge, and PutRichText conditioned on the updated_at we read. A
    /// concurrent save (e.g. the browser) makes the PutRichText fail with
    /// FailedPrecondition; re-read and retry.
    pub(super) async fn append_doc(
        &self,
        r: &DocRef,
        label: &str,
        addition: &Value,
    ) -> Result<(), ToolError> {
        for _ in 0..APPEND_ATTEMPTS {
            let (doc, updated_at) = self.get_doc(r).await?;
            let content = [blocks(&doc), blocks(addition)].concat();
            let merged = json!({ "type": "doc", "content": content });
            match self.put_doc(r, &merged, Some(updated_at)).await {
                Ok(()) => return Ok(()),
                Err(e) if is_code(&e, Code::FailedPrecondition) => {}
                Err(e) => return Err(e.into()),
            }
        }
        fail(format!("{label} kept changing while appending ({APPEND_ATTEMPTS} attempts); nothing was appended. Try again."))
    }
}
