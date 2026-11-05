//! Knowledge operations the MCP tools delegate to. They call the services
//! in-process: agents author markdown, the server derives the link graph.
//!
//! This file is `Knowledge` and its shared helpers. The tools are by area: [`notes`]
//! (create, read, search, tags), [`edit`] (find and replace body blocks),
//! [`relationships`] (links and backlinks), [`daily`], and [`registry`]
//! (`list_structures`). [`doc`] renders and writes a note's doc; [`lines`] maps body
//! lines onto its blocks.

mod daily;
mod doc;
mod edit;
mod lines;
mod notes;
mod registry;
mod relationships;

// The tests build a DocRef to read and write a doc directly.
#[cfg(test)]
pub(in crate::mcp) use doc::{doc_ref, DocRef};

use std::sync::Arc;

use futures::future::try_join_all;
use serde_json::{json, Value};
use tonic::{Code, Request, Response, Status};

use crate::services::entity::EntityService;
use crate::services::richtext::RichTextService;
use crate::services::search::SearchService;
use crate::structures::{self, StructureDef, STRUCTURES};
use calcifer::v1::entity_service_server::EntityService as EntityServiceTrait;
use calcifer::v1::{
    property_value, resolve_entity_request, Entity, GetEntityRequest, ResolveEntityRequest,
};

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

/// The tools' view of the knowledge base: the same services the gRPC surface
/// serves, called directly.
#[derive(Clone)]
pub struct Knowledge {
    entity: Arc<EntityService>,
    richtext: Arc<RichTextService>,
    search: Arc<SearchService>,
}

/// What get_note, query_text_location and edit_note act on: a Note by name or
/// id (or a DailyNote by id), or a DailyNote by ISO day.
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
            Self::Name(name) => missing_note(name),
            Self::Date(date) => format!(
                "No daily note for {}. Use append_to_daily_note to start one.",
                date.trim()
            ),
        }
    }
}

/// What the agent is told when no note has `value` as its name or id.
fn missing_note(value: &str) -> String {
    if is_id(value) {
        format!(
            "No note with id \"{}\". Use search_notes or create_note.",
            value.trim()
        )
    } else {
        format!("No note named \"{value}\". Use search_notes or create_note.")
    }
}

/// Whether `value` is written as an entity id (a UUID).
pub(super) fn is_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value.trim()).is_ok()
}

/// What a name or an id names, when it is a Note's name or anything's id.
pub(super) enum Named {
    /// The one Note with the name, or the entity with the id, of any structure.
    One(Box<Entity>),
    /// No Note has the name, but an entity of this other structure does.
    Other(String),
    /// Nothing has it.
    Nothing,
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

    /// The existing document `key` names: a Note by name or id, a DailyNote by id
    /// or date. A missing note or day is an error, and so is writing to one that
    /// isn't there, not a create: create_note makes a note and
    /// append_to_daily_note starts a day. A name several notes share is refused
    /// (see `named_note`).
    async fn existing_doc(&self, key: &NoteKey<'_>) -> Result<Found, ToolError> {
        let resolved = match key {
            NoteKey::Name(value) => {
                return match self.named_note(value).await? {
                    Named::One(e) if matches!(e.structure_type.as_str(), "Note" | "DailyNote") => {
                        Ok(Found::from_entity(*e))
                    }
                    Named::One(e) => fail(not_a_note(value, &e.structure_type)),
                    Named::Other(kind) => fail(not_a_note(value, &kind)),
                    Named::Nothing => fail(key.missing()),
                };
            }
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

    /// The existing Note `value` names or has as its id. Writing to a note that
    /// isn't there is an error, not a create: create_note is how a note comes to
    /// exist. A name several notes share is refused (see `named_note`).
    pub(super) async fn existing_note(&self, value: &str) -> Result<Entity, ToolError> {
        match self.named_note(value).await? {
            Named::One(e) if e.structure_type == "Note" => Ok(*e),
            Named::One(e) => fail(not_a_note(value, &e.structure_type)),
            Named::Other(kind) => fail(not_a_note(value, &kind)),
            Named::Nothing => fail(missing_note(value)),
        }
    }

    /// What `value` names: an id when it parses as a UUID and some entity has it,
    /// else a Note's name, compared by `fold_name`. Nothing is created. Notes
    /// have no name key, so several can share a name (I-82): that is an error
    /// listing them, for the agent to call again by id, so no tool picks one
    /// silently. A name no Note has is looked up in the other structures, so the
    /// caller can say what it names instead.
    pub(super) async fn named_note(&self, value: &str) -> Result<Named, ToolError> {
        let value = value.trim();
        if is_id(value) {
            match self.entity(value).await {
                Ok(entity) => return Ok(Named::One(Box::new(entity))),
                Err(e) if is_code(&e, Code::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let ids = self.ids_named("Note", value).await?;
        match ids.as_slice() {
            [] => {}
            [id] => return Ok(Named::One(Box::new(self.entity(id).await?))),
            ids => {
                return fail(format!(
                    "Several notes are named \"{value}\":\n{}\nCall again with the id.",
                    self.candidates(ids).await?
                ))
            }
        }
        for s in STRUCTURES.iter().filter(|s| s.type_ != "Note") {
            if !self.ids_named(s.type_, value).await?.is_empty() {
                return Ok(Named::Other(s.type_.to_string()));
            }
        }
        Ok(Named::Nothing)
    }

    /// The ids of every `structure_type` entity named `name` (`fold_name`),
    /// oldest first.
    pub(super) async fn ids_named(
        &self,
        structure_type: &str,
        name: &str,
    ) -> Result<Vec<String>, Status> {
        Ok(self.entity.ids_named(structure_type, name).await?)
    }

    /// How markdown and get_note refer to `entity`: by name, or by id when
    /// another entity of its structure has the same name (only possible without
    /// a name key), since the name wouldn't say which. `[[<id>]]` reads back as
    /// the same entity.
    pub(super) async fn shown_name(&self, entity: &Entity) -> Result<String, Status> {
        let unique =
            structures::structure(&entity.structure_type).is_some_and(StructureDef::unique_names);
        if !unique
            && self
                .ids_named(&entity.structure_type, &entity.name)
                .await?
                .len()
                > 1
        {
            return Ok(entity.id.clone());
        }
        Ok(entity.name.clone())
    }

    /// The entities with `ids`, one line each, for a message that asks the agent
    /// to pick one by id: name, id, and when it was created, oldest first.
    pub(super) async fn candidates(&self, ids: &[String]) -> Result<String, ToolError> {
        let entities = try_join_all(ids.iter().map(|id| self.entity(id))).await?;
        let lines: Vec<String> = entities
            .iter()
            .map(|e| {
                let created = e
                    .created_at
                    .as_ref()
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, 0))
                    .map(|at| format!(", created {}", at.format("%Y-%m-%d %H:%M:%S UTC")))
                    .unwrap_or_default();
                format!("- {} ({}){created}", e.name, e.id)
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

/// What the agent is told when `value` names or is the id of something that
/// isn't a Note.
fn not_a_note(value: &str, kind: &str) -> String {
    format!("\"{}\" is a {kind}, not a Note.", value.trim())
}
