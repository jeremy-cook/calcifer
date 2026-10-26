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

use serde_json::{json, Value};
use tonic::{Code, Request, Response, Status};

use crate::services::entity::EntityService;
use crate::services::richtext::RichTextService;
use crate::services::search::SearchService;
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
    pub(super) async fn existing_note_id(&self, name: &str) -> Result<String, ToolError> {
        match self.resolve_name("Note", name, false).await {
            Ok(res) => Ok(res.entity.map(|e| e.id).unwrap_or_default()),
            Err(e) if is_code(&e, Code::NotFound) => fail(format!(
                "No note named \"{name}\". Use search_notes or create_note."
            )),
            Err(e) => Err(e.into()),
        }
    }
}
