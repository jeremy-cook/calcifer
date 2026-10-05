//! Knowledge operations the MCP tools delegate to. They call the services
//! in-process: agents author markdown, the server derives the link graph.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use futures::future::try_join_all;
use regex::Regex;
use serde_json::{json, Value};
use tonic::{Code, Request, Status};

use super::markdown::{self, ChipTarget, Reference};
use crate::proto::entity_service_server::EntityService as EntityServiceTrait;
use crate::proto::rich_text_service_server::RichTextService as RichTextServiceTrait;
use crate::proto::search_service_server::SearchService as SearchServiceTrait;
use crate::proto::{
    resolve_entity_request, Entity, GetEntityRequest, GetRichTextRequest, ListBacklinksRequest, MatchRange,
    PutRichTextRequest, ResolveEntityRequest, ResolveEntityResponse, SearchMode, SearchRequest,
};
use crate::services::entity::{ts_from_millis, EntityService};
use crate::services::richtext::RichTextService;
use crate::services::search::SearchService;
use crate::structures::{self, PropertyDef, PropertyKind, STRUCTURES};

/// What a failed tool call tells the agent.
#[derive(Debug)]
pub struct ToolError(pub String);

impl From<Status> for ToolError {
    fn from(status: Status) -> Self {
        ToolError(status.message().to_string())
    }
}

type ToolResult = Result<String, ToolError>;

fn fail<T>(message: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(message.into()))
}

fn is_code(status: &Status, code: Code) -> bool {
    status.code() == code
}

fn timestamp_ms(ts: &prost_types::Timestamp) -> i64 {
    ts.seconds * 1000 + i64::from(ts.nanos) / 1_000_000
}

fn empty_doc() -> Value {
    json!({ "type": "doc", "content": [] })
}

fn blocks(doc: &Value) -> Vec<Value> {
    doc["content"].as_array().cloned().unwrap_or_default()
}

/// What a write that links to missing targets tells the agent: each target as
/// it would be written, and how to fix the call.
fn unknown_targets(missing: &[&Reference]) -> ToolError {
    let listed: Vec<String> = missing
        .iter()
        .map(|r| markdown::mention_markdown(&ChipTarget { name: r.name.clone(), structure_type: r.structure_type.clone() }))
        .collect();
    ToolError(format!(
        "Nothing was written: {} link(s) to something that doesn't exist: {}. Fix the name (search_notes finds existing ones) or create the target first: create_note for a note, create_daily_note for a day.",
        listed.len(),
        listed.join(", ")
    ))
}

/// The body doc of an entity: its structure's first declared rich-text property.
pub(super) struct DocRef {
    entity_id: String,
    property_id: String,
}

pub(super) fn doc_ref(structure_type: &str, entity_id: &str) -> Result<DocRef, ToolError> {
    match structures::richtext_properties(structure_type).first() {
        Some(property_id) => Ok(DocRef { entity_id: entity_id.to_string(), property_id: property_id.to_string() }),
        None => fail(format!(
            "Structure \"{structure_type}\" declares no rich-text property, so it has no body to read or write."
        )),
    }
}

/// Where one top-level block lands in the rendered body: its index in the doc's
/// content and its 1-based first and last body line. An empty block renders
/// nothing, so it has no span.
#[derive(Debug, Clone, Copy)]
struct BlockSpan {
    index: usize,
    first: usize,
    last: usize,
}

/// The doc blocks `from..to` that body lines `start..=end` cover; an empty range
/// (end = start - 1) is an insertion point. A line range may start and end on
/// blank lines but never inside a block.
fn block_range(spans: &[BlockSpan], block_count: usize, start: usize, end: usize) -> Result<(usize, usize), ToolError> {
    let total = spans.last().map_or(0, |s| s.last);
    if start > total + 1 {
        return fail(format!(
            "start {start} is outside the note: its body has {total} line(s), so start must be 1 to {}.",
            total + 1
        ));
    }
    if end + 1 < start || end > total {
        return fail(format!(
            "end {end} is outside the note: it must be {} (insert before line {start}) to {total}.",
            start - 1
        ));
    }
    let first = spans.iter().find(|s| s.last >= start);
    if let Some(f) = first.filter(|f| start > f.first) {
        return fail(format!(
            "line {start} is inside the block at lines {}-{}; edits take whole blocks, so start at {}.",
            f.first, f.last, f.first
        ));
    }
    if end + 1 == start {
        let at = first.map_or(block_count, |f| f.index);
        return Ok((at, at));
    }
    let last = spans.iter().rfind(|s| s.first <= end);
    let (Some(first), Some(last)) = (first, last) else {
        return fail(format!("lines {start}-{end} contain no block; to insert there, use end = {}.", start - 1));
    };
    if last.index < first.index {
        return fail(format!("lines {start}-{end} contain no block; to insert there, use end = {}.", start - 1));
    }
    if end < last.last {
        return fail(format!(
            "line {end} is inside the block at lines {}-{}; edits take whole blocks, so end at {}.",
            last.first, last.last, last.last
        ));
    }
    Ok((first.index, last.index + 1))
}

fn is_empty_paragraph(block: &Value) -> bool {
    block["type"] == "paragraph" && block["content"].as_array().is_none_or(Vec::is_empty)
}

/// Blocks `from..to` are about to be replaced from markdown; refuse if one holds
/// a node markdown can't express, which the replacement would silently drop.
fn refuse_inexpressible(spans: &[BlockSpan], blocks: &[Value], from: usize, to: usize) -> Result<(), ToolError> {
    for span in spans.iter().filter(|s| (from..to).contains(&s.index)) {
        if let Some(node_type) = markdown::inexpressible(&blocks[span.index]) {
            let article = if node_type.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
            return fail(format!(
                "lines {}-{} hold {article} {node_type} that markdown can't express; leave them out of the range.",
                span.first, span.last
            ));
        }
    }
    Ok(())
}

static FRONTMATTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^---\r?\n(?:[A-Za-z_][\w-]*:.*\r?\n)+---[ \t]*(?:\r?\n|$)").unwrap());

/// A leading `---` block of `key: value` lines, as get_note writes it. Anything
/// else starting with `---` is note content and is left alone.
fn strip_frontmatter(markdown: &str) -> &str {
    FRONTMATTER.find(markdown).map_or(markdown, |m| &markdown[m.end()..])
}

const APPEND_ATTEMPTS: usize = 3;
const MAX_FOUND_BLOCKS: usize = 20;

/// The tools' view of the knowledge base: the same services the gRPC surface
/// serves, called directly.
#[derive(Clone)]
pub struct Knowledge {
    entity: Arc<EntityService>,
    richtext: Arc<RichTextService>,
    search: Arc<SearchService>,
}

struct BacklinkSource {
    name: String,
    structure_type: String,
    property_ids: Vec<String>,
}

impl Knowledge {
    pub fn new(entity: Arc<EntityService>, richtext: Arc<RichTextService>, search: Arc<SearchService>) -> Self {
        Self { entity, richtext, search }
    }

    /// Get (or, with `create_if_missing`, create) the (structure_type, name)
    /// entity. NotFound when it's missing and `create_if_missing` is false.
    async fn resolve_name(
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
        Ok(self.entity.resolve_entity(Request::new(req)).await?.into_inner())
    }

    pub(super) async fn entity(&self, id: &str) -> Result<Entity, Status> {
        let req = GetEntityRequest { id: id.to_string() };
        let res = self.entity.get_entity(Request::new(req)).await?.into_inner();
        res.entity.ok_or_else(|| Status::internal("GetEntity returned no entity"))
    }

    /// [[wikilinks]] / #tags resolve to canonical entities, keyed by
    /// `Reference::key`, plus the references that name nothing. Only a #tag is
    /// created when missing: it has no body to write and no tool creates one,
    /// while a [[wikilink]] to a missing target is a mistake to report, not a
    /// note to invent.
    async fn resolve_references<'a>(
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
    async fn parse_markdown(&self, markdown: &str) -> Result<Value, ToolError> {
        let parsed = markdown::parse(markdown);
        let (ids, missing) = self.resolve_references(&parsed.references).await?;
        if !missing.is_empty() {
            return Err(unknown_targets(&missing));
        }
        Ok(parsed.resolve(&ids))
    }

    /// Each chip target's current name and structure, fetched in parallel. A
    /// target that no longer exists is left out.
    async fn chip_targets(&self, doc: &Value) -> Result<HashMap<String, ChipTarget>, ToolError> {
        let found = try_join_all(markdown::chip_ids(doc).into_iter().map(|id| async move {
            match self.entity(&id).await {
                Ok(e) => Ok(Some((id, ChipTarget { name: e.name, structure_type: e.structure_type }))),
                Err(e) if is_code(&e, Code::NotFound) => Ok(None),
                Err(e) => Err(e),
            }
        }))
        .await?;
        Ok(found.into_iter().flatten().collect())
    }

    /// The note body as markdown (blocks separated by a blank line), plus the
    /// span of each block so a line range can be mapped back onto blocks.
    async fn render_body(&self, doc: &Value) -> Result<(String, Vec<BlockSpan>), ToolError> {
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
            spans.push(BlockSpan { index, first: line, last });
            parts.push(md);
            line = last + 2;
        }
        Ok((parts.join("\n\n"), spans))
    }

    /// A declared-but-unsaved doc comes back empty at the epoch (see
    /// RichTextService.GetRichText), so `updated_at` is always something a
    /// conditional PutRichText can echo back.
    pub(super) async fn get_doc(&self, r: &DocRef) -> Result<(Value, prost_types::Timestamp), ToolError> {
        let req = GetRichTextRequest { entity_id: r.entity_id.clone(), property_id: r.property_id.clone() };
        let rt = self.richtext.get_rich_text(Request::new(req)).await?.into_inner().rich_text.unwrap_or_default();
        let doc = if rt.doc.is_empty() {
            empty_doc()
        } else {
            serde_json::from_str(&rt.doc).map_err(|e| ToolError(format!("the stored document isn't valid JSON: {e}")))?
        };
        Ok((doc, rt.updated_at.unwrap_or_else(|| ts_from_millis(0))))
    }

    pub(super) async fn put_doc(
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
    async fn append_doc(&self, r: &DocRef, label: &str, addition: &Value) -> Result<(), ToolError> {
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

    /// The entities linking to `target_id`, newest link first. The server returns
    /// one Backlink per link row (self-links excluded), so a source that links
    /// from two properties is folded into one entry here.
    async fn backlink_sources(&self, target_id: &str) -> Result<Vec<BacklinkSource>, ToolError> {
        let req = ListBacklinksRequest { entity_id: target_id.to_string() };
        let res = self.entity.list_backlinks(Request::new(req)).await?.into_inner();
        let mut sources: Vec<(String, BacklinkSource)> = Vec::new();
        for backlink in res.backlinks {
            let Some(source) = backlink.source else { continue };
            let at = match sources.iter().position(|(id, _)| *id == source.id) {
                Some(at) => at,
                None => {
                    let entry = BacklinkSource {
                        name: source.name,
                        structure_type: source.structure_type,
                        property_ids: Vec::new(),
                    };
                    sources.push((source.id, entry));
                    sources.len() - 1
                }
            };
            sources[at].1.property_ids.push(backlink.source_property_id);
        }
        Ok(sources.into_iter().map(|(_, s)| s).collect())
    }

    /// Create a note that doesn't exist yet. An existing note is an error, not an
    /// overwrite: edit_note and append_to_note are how a note's body changes.
    /// Links are checked before the note is created, so a failed call leaves
    /// nothing behind; the note's links to itself are the one target allowed to
    /// be missing.
    pub async fn create_note(&self, name: &str, markdown: &str) -> ToolResult {
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

        let parsed = markdown::parse(strip_frontmatter(markdown));
        let (mut ids, missing) = self.resolve_references(&parsed.references).await?;
        let own_key = Reference { structure_type: "Note".into(), name: name.into() }.key();
        let missing: Vec<&Reference> = missing.into_iter().filter(|r| r.key() != own_key).collect();
        if !missing.is_empty() {
            return Err(unknown_targets(&missing));
        }

        let res = self.resolve_name("Note", name, true).await?;
        // Someone else made it between the check above and now.
        if !res.created {
            return Err(exists());
        }
        let id = res.entity.map(|e| e.id).unwrap_or_default();
        ids.insert(own_key, id.clone());
        // Unconditional: the note was created just above, so it has no body to lose.
        self.put_doc(&doc_ref("Note", &id)?, &parsed.resolve(&ids), None).await?;
        let ent = self.entity(&id).await?;
        let linked = ent.links.iter().map(|l| l.target.as_ref().map_or("", |t| t.structure_type.as_str())).collect::<Vec<_>>();
        let linked = if linked.is_empty() { "none".to_string() } else { linked.join(", ") };
        Ok(format!(
            "Saved note \"{}\" ({id}). Derived links: {} [{linked}]; dates: {}.",
            ent.name,
            ent.links.len(),
            ent.referenced_dates.len()
        ))
    }

    /// The id of the existing note called `name`. Writing to a note that isn't
    /// there is an error, not a create: create_note is how a note comes to exist.
    async fn existing_note_id(&self, name: &str) -> Result<String, ToolError> {
        match self.resolve_name("Note", name, false).await {
            Ok(res) => Ok(res.entity.map(|e| e.id).unwrap_or_default()),
            Err(e) if is_code(&e, Code::NotFound) => fail(format!("No note named \"{name}\". Use search_notes or create_note.")),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn append_to_note(&self, name: &str, markdown: &str) -> ToolResult {
        let id = self.existing_note_id(name).await?;
        let addition = self.parse_markdown(markdown).await?;
        self.append_doc(&doc_ref("Note", &id)?, &format!("Note \"{name}\""), &addition).await?;
        let ent = self.entity(&id).await?;
        Ok(format!("Appended to \"{}\" ({id}); now {} link(s).", ent.name, ent.links.len()))
    }

    /// Where `query` occurs in a note's body, as the whole-block line ranges
    /// edit_note takes (a match inside a list item reports the whole list), with
    /// the version those lines belong to. The match is exact and case-sensitive
    /// against the markdown get_note shows, so the agent quotes text it has read
    /// instead of counting lines.
    pub async fn query_text_location(&self, name: &str, query: &str) -> ToolResult {
        if query.is_empty() {
            return fail("query is empty: say which text to find.");
        }
        let id = self.existing_note_id(name).await?;
        let (doc, updated_at) = self.get_doc(&doc_ref("Note", &id)?).await?;
        let (body, spans) = self.render_body(&doc).await?;
        let line_at = |offset: usize| body[..offset].matches('\n').count() + 1;

        // Each block that holds a match, with how many matches fall in it. A match
        // that spans blocks reports each block it touches.
        let mut hits: Vec<(BlockSpan, usize)> = Vec::new();
        let mut from = 0;
        while let Some(found) = body[from..].find(query) {
            let at = from + found;
            let end = at + query.len();
            let last_char = body[at..end].chars().next_back().map_or(0, char::len_utf8);
            let (first, last) = (line_at(at), line_at(end - last_char));
            for span in spans.iter().filter(|s| s.last >= first && s.first <= last) {
                match hits.iter_mut().find(|(s, _)| s.index == span.index) {
                    Some((_, n)) => *n += 1,
                    None => hits.push((*span, 1)),
                }
            }
            from = end;
        }
        if hits.is_empty() {
            return Ok(format!("No match for {} in \"{name}\".", json!(query)));
        }

        let lines: Vec<&str> = body.split('\n').collect();
        let shown: Vec<String> = hits
            .iter()
            .take(MAX_FOUND_BLOCKS)
            .map(|(s, n)| {
                let matches = if *n == 1 { "1 match".to_string() } else { format!("{n} matches") };
                format!("lines {}-{} ({matches}):\n{}", s.first, s.last, lines[s.first - 1..s.last].join("\n"))
            })
            .collect();
        let more = if hits.len() > shown.len() {
            format!("\n\n({} more block(s) not shown; use a longer query.)", hits.len() - shown.len())
        } else {
            String::new()
        };
        Ok(format!("version: {}\n\n{}{more}", timestamp_ms(&updated_at), shown.join("\n\n")))
    }

    /// Replace lines `start..=end` (1-based) of a note's body with `markdown`. An
    /// empty range (end = start - 1) inserts before line `start`; empty markdown
    /// deletes the range. `version` is the one get_note put in the frontmatter:
    /// line numbers only mean something against the document they were read
    /// from, so a changed note is an error, not a retry.
    pub async fn edit_note(&self, name: &str, start: usize, end: usize, markdown: &str, version: i64) -> ToolResult {
        if start == 0 || version < 0 {
            return fail("start must be at least 1 and version at least 0; nothing was written.");
        }
        let id = self.existing_note_id(name).await?;
        let stale = || ToolError(format!("Note \"{name}\" changed since you read it. Call get_note again and redo the edit."));

        let r = doc_ref("Note", &id)?;
        let (doc, updated_at) = self.get_doc(&r).await?;
        if timestamp_ms(&updated_at) != version {
            return Err(stale());
        }

        let (_, spans) = self.render_body(&doc).await?;
        let mut existing = blocks(&doc);
        // In a note with no visible block, an insert shouldn't land beside empty
        // paragraphs. Anything else unseen stays: it's content all the same.
        if spans.is_empty() {
            existing.retain(|b| !is_empty_paragraph(b));
        }
        let (from, to) = block_range(&spans, existing.len(), start, end)?;
        refuse_inexpressible(&spans, &existing, from, to)?;

        let replacement = if markdown.trim().is_empty() { Vec::new() } else { blocks(&self.parse_markdown(markdown).await?) };
        let mut content = existing[..from].to_vec();
        content.extend(replacement.iter().cloned());
        content.extend(existing[to..].iter().cloned());
        // An empty doc isn't valid TipTap; a note whose blocks were all deleted keeps one empty paragraph.
        if content.is_empty() {
            content.push(json!({ "type": "paragraph" }));
        }
        let merged = json!({ "type": "doc", "content": content });
        match self.put_doc(&r, &merged, Some(updated_at)).await {
            Ok(()) => {}
            Err(e) if is_code(&e, Code::FailedPrecondition) => return Err(stale()),
            Err(e) => return Err(e.into()),
        }

        let ent = self.entity(&id).await?;
        let action = if end + 1 == start {
            format!("Inserted at line {start}")
        } else if replacement.is_empty() {
            format!("Deleted lines {start}-{end}")
        } else {
            format!("Replaced lines {start}-{end}")
        };
        Ok(format!("{action} of \"{}\" ({id}); now {} link(s).", ent.name, ent.links.len()))
    }

    /// The note as markdown under a frontmatter block (name, version,
    /// linked_from). Values are JSON, which is valid YAML, so a name with a colon
    /// or a quote can't break the block. `version` is what edit_note checks its
    /// line numbers against.
    pub async fn get_note(&self, name: &str) -> ToolResult {
        let found = match self.resolve_name("Note", name, false).await {
            Ok(res) => res.entity.unwrap_or_default(),
            Err(e) if is_code(&e, Code::NotFound) => return Ok(format!("No note named \"{name}\". Use search_notes or create_note.")),
            Err(e) => return Err(e.into()),
        };
        let (doc, updated_at) = self.get_doc(&doc_ref("Note", &found.id)?).await?;
        let (body, _) = self.render_body(&doc).await?;
        let linked_from: Vec<String> = self.backlink_sources(&found.id).await?.into_iter().map(|s| s.name).collect();
        Ok(format!(
            "---\nname: {}\nversion: {}\nlinked_from: {}\n---\n{body}",
            json!(found.name),
            timestamp_ms(&updated_at),
            json!(linked_from),
        ))
    }

    pub async fn search_notes(&self, query: &str, limit: u32, mode: SearchMode) -> ToolResult {
        // The server falls back to lexical for semantic/hybrid when embeddings are off.
        let req = SearchRequest { query: query.to_string(), limit, mode: mode as i32 };
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

    pub async fn get_backlinks(&self, name: &str) -> ToolResult {
        let entity = match self.resolve_name("Note", name, false).await {
            Ok(res) => res.entity.unwrap_or_default(),
            Err(e) if is_code(&e, Code::NotFound) => return Ok(format!("No note named \"{name}\".")),
            Err(e) => return Err(e.into()),
        };
        let sources = self.backlink_sources(&entity.id).await?;
        if sources.is_empty() {
            return Ok(format!("Nothing links to \"{name}\" yet."));
        }
        Ok(sources.iter().map(|s| format!("- {}", backlink_label(s))).collect::<Vec<_>>().join("\n"))
    }

    pub async fn link_notes(&self, from: &str, to: &str) -> ToolResult {
        // Links are content-derived, so "linking" = adding a [[to]] mention to `from`.
        self.append_to_note(from, &format!("Related: [[{to}]]")).await?;
        Ok(format!("Linked \"{from}\" -> \"{to}\"."))
    }

    /// Get-or-create the DailyNote for `date` (ISO "YYYY-MM-DD"). The server keeps
    /// one per day and names it for the day.
    async fn resolve_daily_note(&self, date: &str) -> Result<(String, String, bool), ToolError> {
        let req = ResolveEntityRequest {
            structure_type: String::new(),
            key: Some(resolve_entity_request::Key::Date(date.to_string())),
            create_if_missing: true,
        };
        let res = self.entity.resolve_entity(Request::new(req)).await?.into_inner();
        let entity = res.entity.unwrap_or_default();
        Ok((entity.id, entity.name, res.created))
    }

    pub async fn create_daily_note(&self, date: &str) -> ToolResult {
        let (id, name, created) = self.resolve_daily_note(date).await?;
        let outcome = if created { "created" } else { "already existed" };
        Ok(format!("Daily note \"{name}\" ({id}) for {date} {outcome}."))
    }

    pub async fn append_to_daily_note(&self, date: &str, markdown: &str) -> ToolResult {
        // Links first, so a bad one doesn't leave a new, empty day behind.
        let addition = self.parse_markdown(markdown).await?;
        let (id, name, _) = self.resolve_daily_note(date).await?;
        self.append_doc(&doc_ref("DailyNote", &id)?, &format!("Daily note \"{name}\""), &addition).await?;
        let ent = self.entity(&id).await?;
        Ok(format!("Appended to daily note \"{name}\" ({id}); now {} link(s).", ent.links.len()))
    }

    /// The registry lives in `structures.rs`; this only formats it.
    pub fn list_structures(&self) -> ToolResult {
        let lines: Vec<String> = STRUCTURES
            .iter()
            .map(|s| {
                let line = format!("- {}: {}", s.type_, s.description);
                if s.properties.is_empty() {
                    return line;
                }
                let props: Vec<String> = s.properties.iter().map(describe_property).collect();
                format!("{line}\n  properties: {}", props.join(", "))
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

/// Bold each matched range of a plain-text snippet. `**` rather than brackets,
/// so a match never reads as [[wikilink]] syntax. Ranges are UTF-16 offsets;
/// they're taken in order, and where they overlap or run past the snippet only
/// the part not yet bolded counts.
fn mark_matches(snippet: &str, matches: &[MatchRange]) -> String {
    let units: Vec<u16> = snippet.encode_utf16().collect();
    let slice = |from: usize, to: usize| String::from_utf16_lossy(&units[from..to]);
    let mut ranges: Vec<(usize, usize)> = matches.iter().map(|m| (m.start as usize, m.end as usize)).collect();
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

/// The source's name, plus the linking properties unless they're all rich-text docs.
fn backlink_label(source: &BacklinkSource) -> String {
    let docs = structures::richtext_properties(&source.structure_type);
    if source.property_ids.iter().all(|p| docs.contains(&p.as_str())) {
        source.name.clone()
    } else {
        format!("{} (via {})", source.name, source.property_ids.join(", "))
    }
}

/// e.g. "priority (select: none|low|medium|high, default none)", "tags (relations -> Tag)".
fn describe_property(p: &PropertyDef) -> String {
    let kind = match p.kind {
        PropertyKind::Richtext => "richtext",
        PropertyKind::Text => "text",
        PropertyKind::Number => "number",
        PropertyKind::Date => "date",
        PropertyKind::Select => "select",
        PropertyKind::Relation => "relation",
        PropertyKind::Relations => "relations",
    };
    match p.kind {
        PropertyKind::Select => {
            let keys: Vec<&str> = p.options.iter().map(|o| o.key).collect();
            let default = p.default_option.map(|d| format!(", default {d}")).unwrap_or_default();
            format!("{} ({kind}: {}{default})", p.id, keys.join("|"))
        }
        PropertyKind::Relation | PropertyKind::Relations => {
            format!("{} ({kind} -> {})", p.id, p.target_structure.unwrap_or("any"))
        }
        _ => format!("{} ({kind})", p.id),
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn spans() -> Vec<BlockSpan> {
        // Blocks on lines 1, 3, 5-6 and 8.
        [(0, 1, 1), (1, 3, 3), (2, 5, 6), (3, 8, 8)].map(|(index, first, last)| BlockSpan { index, first, last }).to_vec()
    }

    fn range(start: usize, end: usize) -> Result<(usize, usize), String> {
        block_range(&spans(), 4, start, end).map_err(|e| e.0)
    }

    #[test]
    fn block_range_maps_lines_onto_blocks() {
        assert_eq!(range(3, 3), Ok((1, 2)));
        assert_eq!(range(5, 6), Ok((2, 3)));
        assert_eq!(range(2, 4), Ok((1, 2)), "blank edges are fine");
        assert_eq!(range(1, 8), Ok((0, 4)));
        assert_eq!(range(1, 0), Ok((0, 0)), "insert before line 1");
        assert_eq!(range(9, 8), Ok((4, 4)), "start = last + 1 appends");
        assert_eq!(range(4, 3), Ok((2, 2)), "inserting on a blank line goes before the next block");
    }

    #[test]
    fn block_range_rejects_what_cuts_a_block() {
        assert!(range(6, 6).unwrap_err().contains("line 6 is inside the block at lines 5-6; edits take whole blocks, so start at 5"));
        assert!(range(5, 5).unwrap_err().contains("line 5 is inside the block at lines 5-6; edits take whole blocks, so end at 6"));
        assert!(range(2, 2).unwrap_err().contains("contain no block"));
        assert!(range(99, 99).unwrap_err().contains("start 99 is outside"));
        assert!(range(1, 99).unwrap_err().contains("end 99 is outside"));
        assert!(range(3, 1).unwrap_err().contains("end 1 is outside"));
    }

    #[test]
    fn frontmatter_is_stripped_only_when_it_is_key_value_lines() {
        assert_eq!(strip_frontmatter("---\nname: \"x\"\nversion: 3\n---\nBody"), "Body");
        assert_eq!(strip_frontmatter("---\nplain text\n---"), "---\nplain text\n---");
        assert_eq!(strip_frontmatter("Body"), "Body");
    }

    #[test]
    fn matches_are_bolded_by_utf16_offset() {
        let ranges = [MatchRange { start: 3, end: 5 }, MatchRange { start: 9, end: 10 }];
        // "😀" is two UTF-16 units, so "ab" starts at 3.
        assert_eq!(mark_matches("😀 abc d", &ranges[..1]), "😀 **ab**c d");
        assert_eq!(mark_matches("no matches", &[]), "no matches");
        assert_eq!(mark_matches("0123456789", &ranges), "012**34**5678**9**");
    }

    #[test]
    fn overlapping_unordered_and_stray_matches_dont_panic() {
        let range = |start, end| MatchRange { start, end };
        let messy = [range(6, 8), range(1, 4), range(2, 5), range(3, 3), range(9, 99), range(50, 60)];
        assert_eq!(mark_matches("0123456789", &messy), "0**123****4**5**67**8**9**");
        assert_eq!(mark_matches("abc", &[range(2, 1)]), "abc", "an end before its start is empty");
    }
}
