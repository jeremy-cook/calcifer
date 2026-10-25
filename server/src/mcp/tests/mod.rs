//! Tests for the MCP server, by area. The fixture and helpers here are shared.

mod daily;
mod edit;
mod notes;
mod protocol;
mod relationships;
mod search;
mod structures;
mod tags;

use super::tools::{NoteKey, ToolError};
use super::*;
use crate::embed::EmbedHandle;
use crate::proto::entity_service_server::EntityService as _;
use crate::proto::SearchMode;
use crate::proto::{
    resolve_entity_request, AddRelationTargetRequest, CreateEntityRequest, ListEntitiesRequest,
    RenameEntityRequest, ResolveEntityRequest,
};
use crate::services::search::SearchService;
use crate::test_support::{entity_service, memory_pool, richtext_service};
use crate::watch::WatchHub;
use sqlx::SqlitePool;
use std::sync::Arc;
use tonic::Request;

struct Fixture {
    k: Knowledge,
    entity: Arc<crate::services::entity::EntityService>,
}

async fn fixture() -> Fixture {
    let pool: SqlitePool = memory_pool().await;
    let entity = Arc::new(entity_service(pool.clone()));
    let richtext = Arc::new(richtext_service(pool.clone(), WatchHub::new()));
    let search = Arc::new(SearchService::new(pool, EmbedHandle::disabled()));
    Fixture {
        k: Knowledge::new(entity.clone(), richtext, search),
        entity,
    }
}

impl Fixture {
    async fn note_id(&self, name: &str) -> String {
        let req = ResolveEntityRequest {
            structure_type: "Note".into(),
            key: Some(resolve_entity_request::Key::Name(name.into())),
            create_if_missing: false,
        };
        self.entity
            .resolve_entity(Request::new(req))
            .await
            .unwrap()
            .into_inner()
            .entity
            .unwrap()
            .id
    }

    /// The ids of every note called `name`, which no name key keeps unique.
    async fn notes_named(&self, name: &str) -> Vec<String> {
        let req = ListEntitiesRequest {
            structure_type: "Note".into(),
        };
        self.entity
            .list_entities(Request::new(req))
            .await
            .unwrap()
            .into_inner()
            .entities
            .into_iter()
            .filter(|n| n.name == name)
            .map(|n| n.id)
            .collect()
    }

    async fn entity_count(&self) -> usize {
        let req = ListEntitiesRequest {
            structure_type: String::new(),
        };
        self.entity
            .list_entities(Request::new(req))
            .await
            .unwrap()
            .into_inner()
            .entities
            .len()
    }

    /// A note's derived link targets, as sorted "Structure:id" strings.
    async fn link_targets(&self, id: &str) -> Vec<String> {
        let mut targets: Vec<String> = self
            .k
            .entity(id)
            .await
            .unwrap()
            .links
            .iter()
            .map(|l| {
                format!(
                    "{}:{}",
                    l.target.as_ref().unwrap().structure_type,
                    l.target.as_ref().unwrap().id
                )
            })
            .collect();
        targets.sort();
        targets
    }

    async fn create(&self, structure_type: &str, name: &str) {
        let req = CreateEntityRequest {
            structure_type: structure_type.into(),
            name: Some(name.into()),
            properties: Default::default(),
            facets: vec![],
        };
        self.entity.create_entity(Request::new(req)).await.unwrap();
    }
}

/// get_note's body: everything after the frontmatter's closing "---".
fn body(note: &str) -> &str {
    &note[note.find("\n---\n").unwrap() + 5..]
}

/// get_note's frontmatter version, the token edit_note checks line numbers against.
fn version(note: &str) -> i64 {
    note.lines()
        .find_map(|l| l.strip_prefix("version: "))
        .unwrap()
        .parse()
        .unwrap()
}

fn err(result: Result<String, ToolError>) -> String {
    result.expect_err("expected the tool to fail").0
}

/// Create a note and store `content` as its doc, as the browser would.
async fn note_with_blocks(f: &Fixture, name: &str, content: Value) -> tools::DocRef {
    f.k.create_note(name, "", None).await.unwrap();
    let r = tools::doc_ref("Note", &f.note_id(name).await).unwrap();
    let (_, updated_at) = f.k.get_doc(&r).await.unwrap();
    f.k.put_doc(
        &r,
        &json!({ "type": "doc", "content": content }),
        Some(updated_at),
    )
    .await
    .unwrap();
    r
}

/// get_note's frontmatter keys, in order.
fn frontmatter_keys(note: &str) -> Vec<&str> {
    note.strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .unwrap()
        .lines()
        .map(|l| l.split_once(':').unwrap().0)
        .collect()
}

/// The entity's `tags` property, as target ids.
fn tag_ids(entity: &crate::proto::Entity) -> Vec<String> {
    use crate::proto::property_value::Value;
    match entity.properties.get("tags").and_then(|v| v.value.as_ref()) {
        Some(Value::Relations(list)) => list.refs.iter().map(|r| r.id.clone()).collect(),
        _ => vec![],
    }
}

async fn tag_id(f: &Fixture, name: &str) -> String {
    let req = ResolveEntityRequest {
        structure_type: "Tag".into(),
        key: Some(resolve_entity_request::Key::Name(name.into())),
        create_if_missing: false,
    };
    f.entity
        .resolve_entity(Request::new(req))
        .await
        .unwrap()
        .into_inner()
        .entity
        .unwrap()
        .id
}

fn tag_args(names: &[&str]) -> Vec<String> {
    names.iter().map(std::string::ToString::to_string).collect()
}

/// The status `POST /mcp` answers through the local-only layers, sent with
/// these headers and a ping as the body.
async fn post_status(f: &Fixture, headers: &[(&str, &str)]) -> StatusCode {
    use axum::body::Body;
    use tower::ServiceExt;

    let app = crate::security::local::mcp(router(f.k.clone()), "127.0.0.1:8081".parse().unwrap());
    let mut req = axum::http::Request::post("/mcp");
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let body = Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    app.oneshot(req.body(body).unwrap()).await.unwrap().status()
}
