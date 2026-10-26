//! Tests for `EntityService`, by topic. The helpers here are shared by all of them.

mod concurrency;
mod create;
mod daily_notes;
mod delete;
mod facets;
mod list;
mod names;
mod relation_targets;
mod relationships;
mod resolve;
mod set_property;
mod watch;

use std::collections::HashMap;

use futures::stream::BoxStream;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tonic::{Request, Response, Status};

use super::construct::{date_properties, new_entity};
use super::load::load_entities;
use super::lookup::sort_facets;
use super::watch::watch_stream;
use super::EntityService;
use crate::embed::EmbedHandle;
use crate::structures::{self};
use crate::watch::WatchHub;
use calcifer::v1::{
    entity_service_server::EntityService as EntityServiceTrait, property_value,
    resolve_entity_request, watch_entities_response, AddRelationTargetRequest,
    AddRelationshipRequest, AttachFacetRequest, Backlink, CreateEntityRequest, DeleteEntityRequest,
    DetachFacetRequest, Entity, EntityRef, EntityRefList, ListBacklinksRequest,
    ListEntitiesRequest, PropertyValue, RemoveRelationTargetRequest, RemoveRelationshipRequest,
    RenameEntityRequest, ResolveEntityRequest, ResolveEntityResponse, SetEntityPropertyRequest,
    WatchEntitiesRequest, WatchEntitiesResponse,
};

use super::*;
use crate::test_support::{
    entity_service, memory_pool, note, richtext_service, tag, todo, with_facets, ScratchDb,
};
use calcifer::v1::rich_text_service_server::RichTextService as _;
use calcifer::v1::PutRichTextRequest;
use futures::StreamExt;

async fn create(svc: &EntityService, req: CreateEntityRequest) -> Entity {
    try_create(svc, req).await.expect("create")
}

async fn try_create(svc: &EntityService, req: CreateEntityRequest) -> Result<Entity, Status> {
    svc.create_entity(Request::new(req))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
}

/// A Create request with no name, so the server picks the default.
fn untitled(structure_type: &str) -> CreateEntityRequest {
    CreateEntityRequest {
        structure_type: structure_type.to_string(),
        name: None,
        properties: HashMap::new(),
        facets: vec![],
    }
}

async fn rename(svc: &EntityService, id: &str, name: &str) -> Result<Entity, Status> {
    svc.rename_entity(Request::new(RenameEntityRequest {
        id: id.to_string(),
        name: name.to_string(),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

async fn entity_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM entities")
        .fetch_one(pool)
        .await
        .expect("count entities")
}

fn millis(ts: Option<prost_types::Timestamp>) -> i64 {
    let ts = ts.expect("timestamp");
    ts.seconds * 1000 + i64::from(ts.nanos) / 1_000_000
}

async fn key_row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM entity_keys")
        .fetch_one(pool)
        .await
        .expect("count keys")
}

fn select_value(entity: &Entity, property_id: &str) -> Option<String> {
    entity
        .properties
        .get(property_id)
        .and_then(|v| match &v.value {
            Some(property_value::Value::Select(key)) => Some(key.clone()),
            _ => None,
        })
}

/// `req` with `property_id` set to `value`, replacing any earlier value.
fn with_value(
    mut req: CreateEntityRequest,
    property_id: &str,
    value: property_value::Value,
) -> CreateEntityRequest {
    req.properties.insert(
        property_id.to_string(),
        PropertyValue { value: Some(value) },
    );
    req
}

fn with_select(req: CreateEntityRequest, property_id: &str, key: &str) -> CreateEntityRequest {
    with_value(
        req,
        property_id,
        property_value::Value::Select(key.to_string()),
    )
}

fn with_relations(
    req: CreateEntityRequest,
    property_id: &str,
    refs: &[(&str, &str)],
) -> CreateEntityRequest {
    let value = tags_value(refs).expect("relations value");
    with_value(req, property_id, value)
}

/// (target_id, target_structure) of an entity's links from `property_id`, sorted.
fn link_targets(entity: &Entity, property_id: &str) -> Vec<(String, String)> {
    let mut targets: Vec<_> = entity
        .links
        .iter()
        .filter(|l| l.source_property_id == property_id)
        .filter_map(|l| l.target.as_ref())
        .map(|t| (t.id.clone(), t.structure_type.clone()))
        .collect();
    targets.sort();
    targets
}

/// Store `value` under `property_id` directly, as a write before I-79 could
/// have left an undeclared one. Derives no links.
async fn store_legacy_value(
    pool: &SqlitePool,
    entity_id: &str,
    property_id: &str,
    value: property_value::Value,
) {
    let value = PropertyValue { value: Some(value) };
    sqlx::query("INSERT INTO properties (entity_id, property_id, value_blob) VALUES (?, ?, ?)")
        .bind(entity_id)
        .bind(property_id)
        .bind(prost::Message::encode_to_vec(&value))
        .execute(pool)
        .await
        .expect("store property");
}

/// Store a `relations` value naming `target` under `property_id`, with its
/// link row, as `store_legacy_value` does.
async fn store_legacy_relations(
    pool: &SqlitePool,
    entity_id: &str,
    property_id: &str,
    target: &Entity,
) {
    let value = tags_value(&[(&target.id, &target.structure_type)]).expect("relations value");
    store_legacy_value(pool, entity_id, property_id, value).await;
    store_legacy_link(pool, entity_id, property_id, target).await;
}

/// The link row a relation value naming `target` under `property_id` derives.
async fn store_legacy_link(pool: &SqlitePool, entity_id: &str, property_id: &str, target: &Entity) {
    sqlx::query(
        "INSERT INTO links (entity_id, link_id, target_id, target_structure, source_property_id, created_at)
         VALUES (?, ?, ?, ?, ?, 0)",
    )
    .bind(entity_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&target.id)
    .bind(&target.structure_type)
    .bind(property_id)
    .execute(pool)
    .await
    .expect("store link");
}

async fn delete(svc: &EntityService, id: &str) {
    svc.delete_entity(Request::new(DeleteEntityRequest { id: id.to_string() }))
        .await
        .expect("delete");
}

/// The target ids of a stored `relations` value, in stored order.
fn relation_ids(entity: &Entity, property_id: &str) -> Option<Vec<String>> {
    match entity.properties.get(property_id)?.value.as_ref()? {
        property_value::Value::Relations(list) => {
            Some(list.refs.iter().map(|r| r.id.clone()).collect())
        }
        other => panic!("expected relations, got {other:?}"),
    }
}

/// A to-do tagged with `tags`, created a few milliseconds in the past so a
/// later bump of `updated_at` is visible.
async fn tagged_todo(svc: &EntityService, tags: &[&Entity]) -> Entity {
    let refs: Vec<(&str, &str)> = tags.iter().map(|t| (t.id.as_str(), "Tag")).collect();
    let todo = create(svc, with_relations(todo("Water plants"), "tags", &refs)).await;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    todo
}

async fn set_property(
    svc: &EntityService,
    entity_id: &str,
    property_id: &str,
    value: Option<property_value::Value>,
) -> Result<Entity, Status> {
    svc.set_entity_property(Request::new(SetEntityPropertyRequest {
        entity_id: entity_id.to_string(),
        property_id: property_id.to_string(),
        value: value.map(|value| PropertyValue { value: Some(value) }),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "fills the Option<Value> of a property literal"
)]
fn select(key: &str) -> Option<property_value::Value> {
    Some(property_value::Value::Select(key.to_string()))
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "fills the Option<Value> of a property literal"
)]
fn tags_value(refs: &[(&str, &str)]) -> Option<property_value::Value> {
    Some(property_value::Value::Relations(EntityRefList {
        refs: refs
            .iter()
            .map(|(id, structure_type)| EntityRef {
                id: id.to_string(),
                structure_type: structure_type.to_string(),
            })
            .collect(),
    }))
}

/// The DailyNote for `date`, created through Resolve (DailyNote isn't creatable).
async fn daily_note(svc: &EntityService, date: &str) -> Entity {
    resolve(svc, "", by_date(date), true)
        .await
        .expect("resolve daily note")
        .entity
        .expect("entity")
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "fills the Option<Value> of a property literal"
)]
fn date(d: &str) -> Option<property_value::Value> {
    Some(property_value::Value::Date(d.to_string()))
}

async fn fts_name(pool: &SqlitePool, entity_id: &str) -> String {
    sqlx::query_scalar("SELECT name FROM entity_fts WHERE entity_id = ?")
        .bind(entity_id)
        .fetch_one(pool)
        .await
        .expect("fts row")
}

/// The property ids stored in `properties` for `entity_id`, sorted.
async fn stored_property_ids(pool: &SqlitePool, entity_id: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT property_id FROM properties WHERE entity_id = ? ORDER BY property_id",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .expect("property rows")
}

/// The entity's stored `entity_keys` value for `key`, if any.
async fn stored_key(pool: &SqlitePool, entity_id: &str, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM entity_keys WHERE entity_id = ? AND key = ?")
        .bind(entity_id)
        .bind(key)
        .fetch_optional(pool)
        .await
        .expect("key row")
}

async fn stored_date_key(pool: &SqlitePool, entity_id: &str) -> Option<String> {
    stored_key(pool, entity_id, "date").await
}

async fn resolve(
    svc: &EntityService,
    structure_type: &str,
    key: resolve_entity_request::Key,
    create_if_missing: bool,
) -> Result<ResolveEntityResponse, Status> {
    svc.resolve_entity(Request::new(ResolveEntityRequest {
        structure_type: structure_type.to_string(),
        key: Some(key),
        create_if_missing,
    }))
    .await
    .map(Response::into_inner)
}

fn by_date(d: &str) -> resolve_entity_request::Key {
    resolve_entity_request::Key::Date(d.to_string())
}

fn by_name(name: &str) -> resolve_entity_request::Key {
    resolve_entity_request::Key::Name(name.to_string())
}

/// The ids of the entities Upserted on `rx` so far.
fn upserted_ids(rx: &mut tokio::sync::broadcast::Receiver<WatchEntitiesResponse>) -> Vec<String> {
    let mut ids = vec![];
    while let Ok(event) = rx.try_recv() {
        if let Some(watch_entities_response::Event::Upserted(e)) = event.event {
            ids.push(e.id);
        }
    }
    ids
}

/// Put `content` on `entity_id` as a paragraph of the given inline nodes.
async fn put_content(pool: &SqlitePool, entity_id: &str, nodes: &[String]) {
    let doc = format!(
        r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{}]}}]}}"#,
        nodes.join(",")
    );
    richtext_service(pool.clone(), WatchHub::new())
        .put_rich_text(Request::new(PutRichTextRequest {
            entity_id: entity_id.to_string(),
            property_id: "content".to_string(),
            doc,
            expected_updated_at: None,
        }))
        .await
        .expect("put content");
}

fn mention(node_type: &str, target: &Entity) -> String {
    format!(
        r#"{{"type":"{node_type}","attrs":{{"id":"{}","structureType":"{}"}}}}"#,
        target.id, target.structure_type
    )
}

fn date_chip(date: &str) -> String {
    format!(r#"{{"type":"dateChip","attrs":{{"date":"{date}"}}}}"#)
}

/// Ids in List's order before batching: the query List ran for its ids.
async fn listed_ids(pool: &SqlitePool, structure_type: Option<&str>) -> Vec<String> {
    match structure_type {
        None => sqlx::query_scalar("SELECT id FROM entities ORDER BY updated_at DESC, id")
            .fetch_all(pool)
            .await
            .expect("ids"),
        Some(s) => sqlx::query_scalar(
            "SELECT id FROM entities WHERE structure_type = ? ORDER BY updated_at DESC, id",
        )
        .bind(s)
        .fetch_all(pool)
        .await
        .expect("ids"),
    }
}

async fn list_backlinks_of(svc: &EntityService, id: &str) -> Result<Vec<Backlink>, Status> {
    svc.list_backlinks(Request::new(ListBacklinksRequest {
        entity_id: id.to_string(),
    }))
    .await
    .map(|r| r.into_inner().backlinks)
}

/// Pin the created_at of `source`'s link rows from `property_id`.
async fn set_link_created_at(pool: &SqlitePool, source: &str, property_id: &str, millis: i64) {
    sqlx::query("UPDATE links SET created_at = ? WHERE entity_id = ? AND source_property_id = ?")
        .bind(millis)
        .bind(source)
        .bind(property_id)
        .execute(pool)
        .await
        .expect("set link created_at");
}

/// (source id, source property id, created_at millis) of each backlink, in order.
fn backlink_rows(backlinks: &[Backlink]) -> Vec<(String, String, i64)> {
    backlinks
        .iter()
        .map(|b| {
            (
                b.source.as_ref().expect("source").id.clone(),
                b.source_property_id.clone(),
                millis(b.created_at),
            )
        })
        .collect()
}

type EventStream = BoxStream<'static, Result<WatchEntitiesResponse, Status>>;

/// Open a Watch stream on `svc` through the RPC.
async fn open_watch(svc: &EntityService) -> EventStream {
    svc.watch_entities(Request::new(WatchEntitiesRequest {}))
        .await
        .expect("watch")
        .into_inner()
}

/// The stream's next message, failing the test if none comes within a second.
async fn next_event(stream: &mut EventStream) -> WatchEntitiesResponse {
    tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
        .await
        .expect("an event within a second")
        .expect("stream still open")
        .expect("event, not an error")
}

/// The sorted entity ids of a snapshot event.
fn snapshot_ids(event: &WatchEntitiesResponse) -> Vec<String> {
    let Some(watch_entities_response::Event::Snapshot(snapshot)) = &event.event else {
        panic!("expected a snapshot, got {event:?}");
    };
    let mut ids: Vec<String> = snapshot.entities.iter().map(|e| e.id.clone()).collect();
    ids.sort();
    ids
}

/// The id of an upserted event.
fn upserted_id(event: &WatchEntitiesResponse) -> &str {
    let Some(watch_entities_response::Event::Upserted(entity)) = &event.event else {
        panic!("expected an upsert, got {event:?}");
    };
    &entity.id
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

async fn attach(svc: &EntityService, entity_id: &str, facet: &str) -> Result<Entity, Status> {
    svc.attach_facet(Request::new(AttachFacetRequest {
        entity_id: entity_id.to_string(),
        facet: facet.to_string(),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

async fn detach(svc: &EntityService, entity_id: &str, facet: &str) -> Result<Entity, Status> {
    svc.detach_facet(Request::new(DetachFacetRequest {
        entity_id: entity_id.to_string(),
        facet: facet.to_string(),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

async fn facet_row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM entity_facets")
        .fetch_one(pool)
        .await
        .expect("count entity_facets")
}

/// The one `upserted` entity on `rx`, and nothing after it.
fn only_upsert(rx: &mut broadcast::Receiver<WatchEntitiesResponse>) -> Entity {
    let event = rx.try_recv().expect("an event");
    let Some(watch_entities_response::Event::Upserted(entity)) = event.event else {
        panic!("expected an upsert, got {event:?}");
    };
    assert!(rx.try_recv().is_err(), "no further events");
    entity
}

async fn add_target(
    svc: &EntityService,
    entity_id: &str,
    property_id: &str,
    target_id: &str,
) -> Result<Entity, Status> {
    svc.add_relation_target(Request::new(AddRelationTargetRequest {
        entity_id: entity_id.to_string(),
        property_id: property_id.to_string(),
        target_id: target_id.to_string(),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

async fn remove_target(
    svc: &EntityService,
    entity_id: &str,
    property_id: &str,
    target_id: &str,
) -> Result<Entity, Status> {
    svc.remove_relation_target(Request::new(RemoveRelationTargetRequest {
        entity_id: entity_id.to_string(),
        property_id: property_id.to_string(),
        target_id: target_id.to_string(),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

/// The (id, structure_type) refs of a `relations` property, in stored order.
fn relation_refs(entity: &Entity, property_id: &str) -> Vec<(String, String)> {
    match entity
        .properties
        .get(property_id)
        .and_then(|v| v.value.as_ref())
    {
        Some(property_value::Value::Relations(list)) => list
            .refs
            .iter()
            .map(|r| (r.id.clone(), r.structure_type.clone()))
            .collect(),
        None => vec![],
        other => panic!("expected relations, got {other:?}"),
    }
}

/// `(entity.id, "Tag")` pairs for `tags`, the shape `relation_refs` and
/// `link_targets` return.
fn tag_refs(tags: &[&Entity]) -> Vec<(String, String)> {
    tags.iter()
        .map(|t| (t.id.clone(), "Tag".to_string()))
        .collect()
}

async fn add_rel(svc: &EntityService, entity_id: &str, target_id: &str) -> Result<Entity, Status> {
    svc.add_relationship(Request::new(AddRelationshipRequest {
        entity_id: entity_id.to_string(),
        target_id: target_id.to_string(),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

async fn remove_rel(
    svc: &EntityService,
    entity_id: &str,
    target_id: &str,
) -> Result<Entity, Status> {
    svc.remove_relationship(Request::new(RemoveRelationshipRequest {
        entity_id: entity_id.to_string(),
        target_id: target_id.to_string(),
    }))
    .await
    .map(|r| r.into_inner().entity.expect("entity"))
}

async fn relationship_row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM relationships")
        .fetch_one(pool)
        .await
        .expect("count relationships")
}

/// Insert a relationship row directly, bypassing `relates_to`.
async fn insert_relationship_row(pool: &SqlitePool, entity_id: &str, target_id: &str) {
    sqlx::query("INSERT INTO relationships (entity_id, target_id, created_at) VALUES (?, ?, 1)")
        .bind(entity_id)
        .bind(target_id)
        .execute(pool)
        .await
        .expect("insert relationship row");
}
