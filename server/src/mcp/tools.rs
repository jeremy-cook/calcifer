//! Knowledge operations the MCP tools delegate to. They call the services
//! in-process: agents author markdown, the server derives the link graph.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use futures::future::try_join_all;
use regex::Regex;
use serde_json::{json, Value};
use tonic::{Code, Request, Response, Status};

use super::markdown::{self, ChipTarget, Reference};
use crate::proto::entity_service_server::EntityService as EntityServiceTrait;
use crate::proto::rich_text_service_server::RichTextService as RichTextServiceTrait;
use crate::proto::search_service_server::SearchService as SearchServiceTrait;
use crate::proto::{
    property_value, resolve_entity_request, AddRelationTargetRequest, AddRelationshipRequest,
    Entity, EntityRef, EntityRefList, GetEntityRequest, GetRichTextRequest, ListBacklinksRequest,
    ListEntitiesRequest, MatchRange, PropertyValue, PutRichTextRequest,
    RemoveRelationTargetRequest, RemoveRelationshipRequest, ResolveEntityRequest,
    ResolveEntityResponse, SearchMode, SearchRequest, SetEntityPropertyRequest,
};
use crate::services::entity::{check_name_chars, ts_from_millis, EntityService};
use crate::services::richtext::RichTextService;
use crate::services::search::SearchService;
use crate::structures::{self, PropertyDef, PropertyKind, FACETS, STRUCTURES};

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
fn block_range(
    spans: &[BlockSpan],
    block_count: usize,
    start: usize,
    end: usize,
) -> Result<(usize, usize), ToolError> {
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
        return fail(format!(
            "lines {start}-{end} contain no block; to insert there, use end = {}.",
            start - 1
        ));
    };
    if last.index < first.index {
        return fail(format!(
            "lines {start}-{end} contain no block; to insert there, use end = {}.",
            start - 1
        ));
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
/// a node or formatting markdown can't express, which the replacement would
/// silently drop.
fn refuse_inexpressible(
    spans: &[BlockSpan],
    blocks: &[Value],
    from: usize,
    to: usize,
) -> Result<(), ToolError> {
    for span in spans.iter().filter(|s| (from..to).contains(&s.index)) {
        if let Some(what) = markdown::inexpressible(&blocks[span.index]) {
            return fail(format!(
                "lines {}-{} hold {what} that markdown can't express; leave them out of the range.",
                span.first, span.last
            ));
        }
    }
    Ok(())
}

static FRONTMATTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^---\r?\n(?:[A-Za-z_][\w-]*:.*\r?\n)+---[ \t]*(?:\r?\n|$)").unwrap()
});

/// A leading `---` block of `key: value` lines, as get_note writes it. Anything
/// else starting with `---` is note content and is left alone.
fn strip_frontmatter(markdown: &str) -> &str {
    FRONTMATTER
        .find(markdown)
        .map_or(markdown, |m| &markdown[m.end()..])
}

/// The `tags:` line of a leading frontmatter block, as get_note writes it: a
/// JSON list of tag names. `None` when there is no block or it has no such line.
/// A value that isn't a list of strings is an error, so a copied note never
/// loses its tags without saying so.
fn frontmatter_tags(markdown: &str) -> Result<Option<Vec<String>>, ToolError> {
    let Some(block) = FRONTMATTER.find(markdown) else {
        return Ok(None);
    };
    let Some(value) = block
        .as_str()
        .lines()
        .find_map(|line| line.strip_prefix("tags:"))
    else {
        return Ok(None);
    };
    match serde_json::from_str::<Vec<String>>(value.trim()) {
        Ok(tags) => Ok(Some(tags)),
        Err(_) => fail(format!(
            "The frontmatter line `tags:{value}` must be a JSON list of tag names, like tags: [\"ml\",\"papers\"]. Nothing was written."
        )),
    }
}

const APPEND_ATTEMPTS: usize = 3;
const MAX_FOUND_BLOCKS: usize = 20;

/// The link scope the server derives from an entity's direct relationships (ADR 12).
const RELATED_SCOPE: &str = "@related";

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

/// What get_note, query_text_location and edit_note act on: a Note by name or a
/// DailyNote by ISO day.
pub enum NoteKey<'a> {
    Name(&'a str),
    Date(&'a str),
}

impl<'a> NoteKey<'a> {
    /// Exactly one of `name` and `date`.
    pub fn from_args(name: Option<&'a str>, date: Option<&'a str>) -> Result<Self, ToolError> {
        match (name, date) {
            (Some(name), None) => Ok(Self::Name(name)),
            (None, Some(date)) => Ok(Self::Date(date)),
            (Some(_), Some(_)) => {
                fail("Give either name (a note) or date (a daily note, yyyy-MM-dd), not both.")
            }
            (None, None) => fail(
                "Give either name (a note) or date (a daily note, yyyy-MM-dd); neither was given.",
            ),
        }
    }

    /// What the agent is told when nothing is there.
    fn missing(&self) -> String {
        match self {
            Self::Name(name) => {
                format!("No note named \"{name}\". Use search_notes or create_note.")
            }
            Self::Date(date) => format!(
                "No daily note for {}. Use append_to_daily_note to start one.",
                date.trim()
            ),
        }
    }
}

/// A document `NoteKey` found: a Note or a DailyNote.
struct Found {
    id: String,
    name: String,
    structure_type: String,
    /// The ISO day of a DailyNote.
    date: Option<String>,
    entity: Entity,
}

impl Found {
    fn from_entity(entity: Entity) -> Self {
        let date = match entity.properties.get("date").and_then(|v| v.value.as_ref()) {
            Some(property_value::Value::Date(d)) => Some(d.clone()),
            _ => None,
        };
        Self {
            id: entity.id.clone(),
            name: entity.name.clone(),
            structure_type: entity.structure_type.clone(),
            date,
            entity,
        }
    }

    /// How messages name it: `Note "X"` or `Daily note "October 24, 2026"`.
    fn label(&self) -> String {
        match self.structure_type.as_str() {
            "DailyNote" => format!("Daily note \"{}\"", self.name),
            _ => format!("Note \"{}\"", self.name),
        }
    }
}

impl Knowledge {
    pub fn new(
        entity: Arc<EntityService>,
        richtext: Arc<RichTextService>,
        search: Arc<SearchService>,
    ) -> Self {
        Self {
            entity,
            richtext,
            search,
        }
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
        Ok(self
            .entity
            .resolve_entity(Request::new(req))
            .await?
            .into_inner())
    }

    pub(super) async fn entity(&self, id: &str) -> Result<Entity, Status> {
        let req = GetEntityRequest { id: id.to_string() };
        let res = self
            .entity
            .get_entity(Request::new(req))
            .await?
            .into_inner();
        res.entity
            .ok_or_else(|| Status::internal("GetEntity returned no entity"))
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
    pub(super) async fn get_doc(
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

    /// Create a note that doesn't exist yet. An existing note is an error, not an
    /// overwrite: edit_note and append_to_note are how a note's body changes.
    /// Links are checked before the note is created, so a failed call leaves
    /// nothing behind; the note's links to itself are the one target allowed to
    /// be missing. Each of `tags` is resolved, or created, as a Tag and set as
    /// the note's `tags` property; a body #tag only references its Tag. With no
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

        let res = self.resolve_name("Note", name, true).await?;
        // Someone else made it between the check above and now.
        if !res.created {
            return Err(exists());
        }
        let id = res.entity.map(|e| e.id).unwrap_or_default();
        ids.insert(own_key, id.clone());
        // Unconditional: the note was created just above, so it has no body to lose.
        let body = doc_ref("Note", &id)?;
        self.put_doc(&body, &parsed.resolve(&ids), None).await?;
        if !tag_ids.is_empty() {
            self.set_tags(&id, tag_ids).await?;
        }
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
    async fn resolve_tags(&self, tags: &[String]) -> Result<Vec<String>, ToolError> {
        let found = try_join_all(tags.iter().map(|t| self.resolve_name("Tag", t, true))).await?;
        let mut ids: Vec<String> = Vec::new();
        for id in found.into_iter().filter_map(|res| res.entity.map(|e| e.id)) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    async fn set_tags(&self, entity_id: &str, tag_ids: Vec<String>) -> Result<(), Status> {
        let refs = tag_ids
            .into_iter()
            .map(|id| EntityRef {
                id,
                structure_type: "Tag".into(),
            })
            .collect();
        let req = SetEntityPropertyRequest {
            entity_id: entity_id.to_string(),
            property_id: "tags".into(),
            value: Some(PropertyValue {
                value: Some(property_value::Value::Relations(EntityRefList { refs })),
            }),
        };
        self.entity.set_entity_property(Request::new(req)).await?;
        Ok(())
    }

    /// The existing document `key` names. A missing note or day is an error, and
    /// so is writing to one that isn't there, not a create: create_note makes a
    /// note and append_to_daily_note starts a day.
    async fn existing_doc(&self, key: &NoteKey<'_>) -> Result<Found, ToolError> {
        let resolved = match key {
            NoteKey::Name(name) => self.resolve_name("Note", name, false).await,
            NoteKey::Date(date) => {
                let req = ResolveEntityRequest {
                    structure_type: String::new(),
                    key: Some(resolve_entity_request::Key::Date(date.to_string())),
                    create_if_missing: false,
                };
                self.entity
                    .resolve_entity(Request::new(req))
                    .await
                    .map(Response::into_inner)
            }
        };
        match resolved {
            Ok(res) => match res.entity {
                Some(entity) => Ok(Found::from_entity(entity)),
                None => fail(key.missing()),
            },
            Err(e) if is_code(&e, Code::NotFound) => fail(key.missing()),
            Err(e) => Err(e.into()),
        }
    }

    /// The id of the existing note called `name`. Writing to a note that isn't
    /// there is an error, not a create: create_note is how a note comes to exist.
    async fn existing_note_id(&self, name: &str) -> Result<String, ToolError> {
        match self.resolve_name("Note", name, false).await {
            Ok(res) => Ok(res.entity.map(|e| e.id).unwrap_or_default()),
            Err(e) if is_code(&e, Code::NotFound) => fail(format!(
                "No note named \"{name}\". Use search_notes or create_note."
            )),
            Err(e) => Err(e.into()),
        }
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
    async fn tag_names(&self, entity: &Entity) -> Result<Vec<String>, Status> {
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

    /// The names of the entities `entity` holds direct relationships to (its
    /// `@related` links), oldest first, ties by target id as the server orders the
    /// rows it derives them from. A target that no longer exists is skipped.
    async fn related_names(&self, entity: &Entity) -> Result<Vec<String>, Status> {
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

    /// Where `query` occurs in a note's body, as the whole-block line ranges
    /// edit_note takes (a match inside a list item reports the whole list), with
    /// the version those lines belong to. The match is exact and case-sensitive
    /// against the markdown get_note shows, so the agent quotes text it has read
    /// instead of counting lines.
    pub async fn query_text_location(&self, key: NoteKey<'_>, query: &str) -> ToolResult {
        if query.is_empty() {
            return fail("query is empty: say which text to find.");
        }
        let found = self.existing_doc(&key).await?;
        let (doc, updated_at) = self
            .get_doc(&doc_ref(&found.structure_type, &found.id)?)
            .await?;
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
            return Ok(format!(
                "No match for {} in {}.",
                json!(query),
                found.label()
            ));
        }

        let lines: Vec<&str> = body.split('\n').collect();
        let shown: Vec<String> = hits
            .iter()
            .take(MAX_FOUND_BLOCKS)
            .map(|(s, n)| {
                let matches = if *n == 1 {
                    "1 match".to_string()
                } else {
                    format!("{n} matches")
                };
                format!(
                    "lines {}-{} ({matches}):\n{}",
                    s.first,
                    s.last,
                    lines[s.first - 1..s.last].join("\n")
                )
            })
            .collect();
        let more = if hits.len() > shown.len() {
            format!(
                "\n\n({} more block(s) not shown; use a longer query.)",
                hits.len() - shown.len()
            )
        } else {
            String::new()
        };
        Ok(format!(
            "version: {}\n\n{}{more}",
            timestamp_ms(&updated_at),
            shown.join("\n\n")
        ))
    }

    /// Replace lines `start..=end` (1-based) of a note's body with `markdown`. An
    /// empty range (end = start - 1) inserts before line `start`; empty markdown
    /// deletes the range. `version` is the one get_note put in the frontmatter:
    /// line numbers only mean something against the document they were read
    /// from, so a changed note is an error, not a retry.
    pub async fn edit_note(
        &self,
        key: NoteKey<'_>,
        start: usize,
        end: usize,
        markdown: &str,
        version: i64,
    ) -> ToolResult {
        if start == 0 || version < 0 {
            return fail("start must be at least 1 and version at least 0; nothing was written.");
        }
        let found = self.existing_doc(&key).await?;
        let stale = || {
            ToolError(format!(
                "{} changed since you read it. Call get_note again and redo the edit.",
                found.label()
            ))
        };

        let r = doc_ref(&found.structure_type, &found.id)?;
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

        let replacement = if markdown.trim().is_empty() {
            Vec::new()
        } else {
            blocks(&self.parse_markdown(markdown).await?)
        };
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

        let ent = self.entity(&found.id).await?;
        let action = if end + 1 == start {
            format!("Inserted at line {start}")
        } else if replacement.is_empty() {
            format!("Deleted lines {start}-{end}")
        } else {
            format!("Replaced lines {start}-{end}")
        };
        Ok(format!(
            "{action} of {} ({}); now {} wikilink(s).",
            found.label(),
            found.id,
            ent.links.len()
        ))
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
    async fn note_by_name_or_id(&self, value: &str) -> Result<Entity, ToolError> {
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
    async fn relationship_ends(&self, from: &str, to: &str) -> Result<(Entity, Entity), ToolError> {
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

    /// Get-or-create the DailyNote for `date` (ISO "YYYY-MM-DD"). The server keeps
    /// one per day and names it for the day.
    async fn resolve_daily_note(&self, date: &str) -> Result<(String, String, bool), ToolError> {
        let req = ResolveEntityRequest {
            structure_type: String::new(),
            key: Some(resolve_entity_request::Key::Date(date.to_string())),
            create_if_missing: true,
        };
        let res = self
            .entity
            .resolve_entity(Request::new(req))
            .await?
            .into_inner();
        let entity = res.entity.unwrap_or_default();
        Ok((entity.id, entity.name, res.created))
    }

    pub async fn create_daily_note(&self, date: &str) -> ToolResult {
        let (id, name, created) = self.resolve_daily_note(date).await?;
        let outcome = if created {
            "created"
        } else {
            "already existed"
        };
        Ok(format!(
            "Daily note \"{name}\" ({id}) for {date} {outcome}."
        ))
    }

    pub async fn append_to_daily_note(&self, date: &str, markdown: &str) -> ToolResult {
        // Links first, so a bad one doesn't leave a new, empty day behind.
        let addition = self.parse_markdown(markdown).await?;
        let (id, name, _) = self.resolve_daily_note(date).await?;
        self.append_doc(
            &doc_ref("DailyNote", &id)?,
            &format!("Daily note \"{name}\""),
            &addition,
        )
        .await?;
        let ent = self.entity(&id).await?;
        Ok(format!(
            "Appended to daily note \"{name}\" ({id}); now {} wikilink(s).",
            ent.links.len()
        ))
    }

    /// The registry lives in `structures.rs`; this only formats it.
    pub fn list_structures(&self) -> String {
        let lines: Vec<String> = STRUCTURES
            .iter()
            .map(|s| {
                let mut line = format!("- {}: {}", s.type_, s.description);
                if !s.properties.is_empty() {
                    let props: Vec<String> = s.properties.iter().map(describe_property).collect();
                    line += &format!("\n  properties: {}", props.join(", "));
                }
                if !s.facets.is_empty() {
                    line += &format!("\n  facets: {}", s.facets.join(", "));
                }
                if !s.relates_to.is_empty() {
                    line += &format!("\n  relates_to: {}", s.relates_to.join(", "));
                }
                line
            })
            .chain(FACETS.iter().map(|f| {
                let props: Vec<String> = f.properties.iter().map(describe_property).collect();
                let accepted: Vec<&str> = STRUCTURES
                    .iter()
                    .filter(|s| s.facets.contains(&f.id))
                    .map(|s| s.type_)
                    .collect();
                format!(
                    "- facet {} ({}): {}\n  properties: {}\n  accepted by: {}",
                    f.id,
                    f.name,
                    f.description,
                    props.join(", "),
                    accepted.join(", ")
                )
            }))
            .collect();
        lines.join("\n")
    }
}

/// Bold each matched range of a plain-text snippet. `**` rather than brackets,
/// so a match never reads as [[wikilink]] syntax. Ranges are UTF-16 offsets;
/// they're taken in order, and where they overlap or run past the snippet only
/// the part not yet bolded counts.
fn mark_matches(snippet: &str, matches: &[MatchRange]) -> String {
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

/// Whether `entity` holds a direct relationship to `target_id`.
fn is_related(entity: &Entity, target_id: &str) -> bool {
    entity.links.iter().any(|l| {
        l.source_property_id == RELATED_SCOPE
            && l.target.as_ref().is_some_and(|t| t.id == target_id)
    })
}

/// The source's name, plus why it links: `related` for a direct relationship,
/// then `via` the properties that aren't rich-text docs. A source that links only
/// from its docs stays bare.
fn backlink_label(source: &BacklinkSource) -> String {
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
            let default = p
                .default_option
                .map(|d| format!(", default {d}"))
                .unwrap_or_default();
            format!("{} ({kind}: {}{default})", p.id, keys.join("|"))
        }
        PropertyKind::Relation | PropertyKind::Relations => {
            format!(
                "{} ({kind} -> {})",
                p.id,
                p.target_structure.unwrap_or("any")
            )
        }
        _ => format!("{} ({kind})", p.id),
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn spans() -> Vec<BlockSpan> {
        // Blocks on lines 1, 3, 5-6 and 8.
        [(0, 1, 1), (1, 3, 3), (2, 5, 6), (3, 8, 8)]
            .map(|(index, first, last)| BlockSpan { index, first, last })
            .to_vec()
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
        assert_eq!(
            range(4, 3),
            Ok((2, 2)),
            "inserting on a blank line goes before the next block"
        );
    }

    #[test]
    fn block_range_rejects_what_cuts_a_block() {
        assert!(range(6, 6).unwrap_err().contains(
            "line 6 is inside the block at lines 5-6; edits take whole blocks, so start at 5"
        ));
        assert!(range(5, 5).unwrap_err().contains(
            "line 5 is inside the block at lines 5-6; edits take whole blocks, so end at 6"
        ));
        assert!(range(2, 2).unwrap_err().contains("contain no block"));
        assert!(range(99, 99).unwrap_err().contains("start 99 is outside"));
        assert!(range(1, 99).unwrap_err().contains("end 99 is outside"));
        assert!(range(3, 1).unwrap_err().contains("end 1 is outside"));
    }

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

    #[test]
    fn frontmatter_is_stripped_only_when_it_is_key_value_lines() {
        assert_eq!(
            strip_frontmatter("---\nname: \"x\"\nversion: 3\n---\nBody"),
            "Body"
        );
        assert_eq!(
            strip_frontmatter("---\nplain text\n---"),
            "---\nplain text\n---"
        );
        assert_eq!(strip_frontmatter("Body"), "Body");
    }

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
