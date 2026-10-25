//! The agent's door into the knowledge base: an MCP server (streamable HTTP,
//! `POST /mcp`) hosted by the same process as the gRPC-Web API and the UI. Its
//! tools call the services in-process, so an agent's write takes the same path
//! as the browser's and shows up live over `WatchEntities`.
//!
//! Only the request/response half of the transport is implemented: every POST
//! is answered with one JSON body. There are no server-initiated messages, so
//! there is no stream for GET to open.

mod markdown;
mod tools;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::proto::SearchMode;
pub use tools::Knowledge;
use tools::{NoteKey, ToolError};

/// How note markdown refers to other entities; shared by the tools that read or write it.
const LINK_SYNTAX: &str = "[[Name]] links an existing note; [[Structure/Name]] links an existing entity of another structure from list_structures, e.g. [[DailyNote/June 13, 2026]]; #tag references a tag (created if missing); it doesn't tag the note. Pass `tags` to create_note, or use tag_entity, to tag it. To relate two notes without writing a link into either, use link_notes. A link to anything that doesn't exist fails the call and writes nothing, so create the target first. These become real graph links automatically.";

/// Newest first. A client's version is echoed back when we speak it.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const PARSE_ERROR: i64 = -32700;

/// `POST /mcp`. Any other method on the path is answered 405 by the router.
pub fn router(knowledge: Knowledge) -> axum::Router {
    axum::Router::new()
        .route("/mcp", post(post_mcp))
        .with_state(knowledge)
}

async fn post_mcp(State(knowledge): State<Knowledge>, body: Bytes) -> Response {
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return json_response(&error_reply(&Value::Null, PARSE_ERROR, "invalid JSON"));
    };
    // A batch is answered with a batch of the replies; notifications have none.
    let reply = match message {
        Value::Array(messages) if messages.is_empty() => {
            Some(error_reply(&Value::Null, INVALID_REQUEST, "empty batch"))
        }
        Value::Array(messages) => {
            let replies: Vec<Value> = futures::future::join_all(
                messages.into_iter().map(|m| handle_message(&knowledge, m)),
            )
            .await
            .into_iter()
            .flatten()
            .collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        message => handle_message(&knowledge, message).await,
    };
    match reply {
        Some(reply) => json_response(&reply),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn json_response(body: &Value) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn error_reply(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// One JSON-RPC message in, its reply out. Notifications and stray responses
/// (no `method`, or no `id`) get none; anything that isn't an object is an error.
async fn handle_message(knowledge: &Knowledge, message: Value) -> Option<Value> {
    if !message.is_object() {
        return Some(error_reply(
            &Value::Null,
            INVALID_REQUEST,
            "not a JSON-RPC message",
        ));
    }
    let id = message.get("id").cloned();
    let Some(method) = message["method"].as_str() else {
        return id.map(|id| error_reply(&id, INVALID_REQUEST, "not a JSON-RPC request"));
    };
    let id = id?;
    let params = &message["params"];
    let outcome = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(knowledge, params).await,
        _ => Err((METHOD_NOT_FOUND, format!("unknown method {method}"))),
    };
    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_reply(&id, code, &message),
    })
}

fn initialize(params: &Value) -> Value {
    let version = params["protocolVersion"]
        .as_str()
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "calcifer", "version": env!("CARGO_PKG_VERSION") },
    })
}

async fn call_tool(knowledge: &Knowledge, params: &Value) -> Result<Value, (i64, String)> {
    let name = params["name"].as_str().unwrap_or_default();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let outcome = match dispatch(knowledge, name, args).await {
        Ok(outcome) => outcome,
        Err(rejected) => return Err((INVALID_PARAMS, rejected)),
    };
    // A failed tool is a result the agent can read, not a protocol error.
    let (text, is_error) = match outcome {
        Ok(text) => (text, false),
        Err(ToolError(text)) => (text, true),
    };
    Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
}

fn parse_args<T: DeserializeOwned>(tool: &str, args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("invalid arguments for {tool}: {e}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    limit: Option<u32>,
    mode: Option<Mode>,
}

#[derive(Deserialize)]
struct NameArgs {
    name: String,
}

#[derive(Deserialize)]
struct GetNoteArgs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    date: Option<String>,
}

#[derive(Deserialize)]
struct QueryTextArgs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    date: Option<String>,
    query: String,
}

#[derive(Deserialize)]
struct EditArgs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    date: Option<String>,
    start: usize,
    end: usize,
    markdown: String,
    version: i64,
}

#[derive(Deserialize)]
struct NoteArgs {
    name: String,
    markdown: String,
}

#[derive(Deserialize)]
struct CreateNoteArgs {
    name: String,
    markdown: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct TagArgs {
    name: String,
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

#[derive(Deserialize)]
struct LinkArgs {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct DateArgs {
    date: String,
}

#[derive(Deserialize)]
struct DailyArgs {
    date: String,
    markdown: String,
}

/// Runs the named tool. The outer error is a request the server can't read
/// (unknown tool, bad arguments); the inner one is the tool failing.
async fn dispatch(
    k: &Knowledge,
    name: &str,
    args: Value,
) -> Result<Result<String, ToolError>, String> {
    Ok(match name {
        "search_notes" => {
            let a: SearchArgs = parse_args(name, args)?;
            let mode = match a.mode.unwrap_or(Mode::Hybrid) {
                Mode::Lexical => SearchMode::Lexical,
                Mode::Semantic => SearchMode::Semantic,
                Mode::Hybrid => SearchMode::Hybrid,
            };
            match a.limit {
                Some(0) => {
                    return Err("invalid arguments for search_notes: limit must be positive".into())
                }
                limit => k.search_notes(&a.query, limit.unwrap_or(10), mode).await,
            }
        }
        "get_note" => {
            let a: GetNoteArgs = parse_args(name, args)?;
            match NoteKey::from_args(a.name.as_deref(), a.date.as_deref()) {
                Ok(key) => k.get_note(key).await,
                Err(e) => Err(e),
            }
        }
        "query_text_location" => {
            let a: QueryTextArgs = parse_args(name, args)?;
            match NoteKey::from_args(a.name.as_deref(), a.date.as_deref()) {
                Ok(key) => k.query_text_location(key, &a.query).await,
                Err(e) => Err(e),
            }
        }
        "edit_note" => {
            let a: EditArgs = parse_args(name, args)?;
            match NoteKey::from_args(a.name.as_deref(), a.date.as_deref()) {
                Ok(key) => {
                    k.edit_note(key, a.start, a.end, &a.markdown, a.version)
                        .await
                }
                Err(e) => Err(e),
            }
        }
        "create_note" => {
            let a: CreateNoteArgs = parse_args(name, args)?;
            k.create_note(&a.name, &a.markdown, a.tags.as_deref()).await
        }
        "append_to_note" => {
            let a: NoteArgs = parse_args(name, args)?;
            k.append_to_note(&a.name, &a.markdown).await
        }
        "tag_entity" => {
            let a: TagArgs = parse_args(name, args)?;
            k.tag_entity(&a.name, &a.add, &a.remove).await
        }
        "link_notes" => {
            let a: LinkArgs = parse_args(name, args)?;
            k.link_notes(&a.from, &a.to).await
        }
        "unlink_notes" => {
            let a: LinkArgs = parse_args(name, args)?;
            k.unlink_notes(&a.from, &a.to).await
        }
        "get_backlinks" => {
            k.get_backlinks(&parse_args::<NameArgs>(name, args)?.name)
                .await
        }
        "create_daily_note" => {
            k.create_daily_note(&parse_args::<DateArgs>(name, args)?.date)
                .await
        }
        "append_to_daily_note" => {
            let a: DailyArgs = parse_args(name, args)?;
            k.append_to_daily_note(&a.date, &a.markdown).await
        }
        "list_structures" => Ok(k.list_structures()),
        _ => return Err(format!("unknown tool {name}")),
    })
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "every call site builds the description and schema inline"
)]
fn tool(name: &str, description: String, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

fn tool_definitions() -> Vec<Value> {
    let string = json!({ "type": "string" });
    vec![
        tool(
            "search_notes",
            "Search existing notes and tags. Use this before creating, to find and reuse existing notes instead of making duplicates. `mode` picks the ranking: \"lexical\" (keyword/FTS5), \"semantic\" (meaning via embeddings), or \"hybrid\" (both, fused — the default and best for natural-language queries).".into(),
            json!({
                "query": string,
                "limit": { "type": "integer", "exclusiveMinimum": 0 },
                "mode": { "type": "string", "enum": ["lexical", "semantic", "hybrid"] },
            }),
            &["query"],
        ),
        tool(
            "get_note",
            format!("Read a note by name, or a daily note by date (yyyy-MM-dd); give one of the two. The result is YAML frontmatter followed by the markdown body: name, version, tags and related for a note (`related` lists the notes it's directly related to; change it with link_notes and unlink_notes); name, version and date for a daily note, which has no tags or related. A day with no daily note is an error: use append_to_daily_note to start one. Use get_backlinks for the notes that link to it. Links are written with each target's current name: {LINK_SYNTAX} create_note reads the frontmatter's tags when given no `tags` argument, so the result can be copied into a new note; it ignores `related`, so the copy isn't related to anything."),
            json!({ "name": string, "date": string }),
            &[],
        ),
        tool(
            "query_text_location",
            "Find where text occurs in an existing note (by name) or daily note (by date, yyyy-MM-dd); give one of the two. Returns the note's version and, for each block containing a match, its line range and text. The ranges are whole blocks, ready to pass to edit_note as start and end, so you never have to count lines. The match is exact and case-sensitive against the markdown get_note shows (links as [[Name]]); quote enough text to be unique.".into(),
            json!({ "name": string, "date": string, "query": string }),
            &["query"],
        ),
        tool(
            "edit_note",
            format!("Replace a range of lines in the body of an existing note (by name) or daily note (by date, yyyy-MM-dd); give one of the two. Get the lines and version from query_text_location, or read the note with get_note: lines are counted in the body only, from 1 at the first line after the closing --- of the frontmatter, and version is the frontmatter's version. A paragraph, a heading, a whole list and a whole code block are each one block, and blocks are separated by a blank line; start and end must not fall inside a block. Lines start..end (inclusive) are replaced by the markdown; empty markdown deletes them; end = start - 1 inserts before line start (start = last line + 1 appends). A <!-- calcifer:<type> --> line stands for content markdown can't express (a table, say), and formatting markdown can't express (underline, colour, alignment, ...) is shown as plain text: a range that covers either is refused, so edit around it. Fails if the note changed since you read it: read again and redo the edit. In the markdown, {LINK_SYNTAX}"),
            json!({
                "name": string,
                "date": string,
                "start": { "type": "integer", "exclusiveMinimum": 0 },
                "end": { "type": "integer", "minimum": 0 },
                "markdown": string,
                "version": { "type": "integer", "minimum": 0 },
            }),
            &["start", "end", "markdown", "version"],
        ),
        tool(
            "create_note",
            format!("Create a new note. Fails if a note with that name already exists; change an existing note with edit_note or append_to_note. In the markdown body, {LINK_SYNTAX} `tags` lists tag names to tag the note with; each Tag is created if missing. Without a `tags` argument, a `tags:` line in leading frontmatter (as get_note writes it) is used. Search first to avoid duplicates. Copying get_note output into a new note drops its <!-- calcifer:<type> --> placeholder blocks and any formatting markdown can't express."),
            json!({ "name": string, "markdown": string, "tags": { "type": "array", "items": string } }),
            &["name", "markdown"],
        ),
        tool(
            "append_to_note",
            format!("Append markdown to an existing note, growing it across turns. Fails if there is no note with that name; use create_note to make one. In the appended text, {LINK_SYNTAX}"),
            json!({ "name": string, "markdown": string }),
            &["name", "markdown"],
        ),
        tool(
            "tag_entity",
            "Add or remove tags on an existing note (only notes have tags). `add` lists tag names to tag it with, each Tag created if missing; `remove` lists tag names to untag, and a tag the note doesn't have is ignored. Give at least one. Returns the note's tags afterwards. A #tag in a note's body only references the Tag; this is how a note is tagged.".into(),
            json!({
                "name": string,
                "add": { "type": "array", "items": string },
                "remove": { "type": "array", "items": string },
            }),
            &["name"],
        ),
        tool(
            "link_notes",
            "Relate one note to another without touching either note's text (a direct relationship). `from` and `to` are each a note's name or id; if several notes share a name, the call fails and lists their ids. Relating twice is harmless. Only notes relate to notes.".into(),
            json!({ "from": string, "to": string }),
            &["from", "to"],
        ),
        tool(
            "unlink_notes",
            "Remove the direct relationship from one note to another (made with link_notes) without touching either note's text. `from` and `to` are each a note's name or id; if several notes share a name, the call fails and lists their ids. Removing a relationship that doesn't exist is harmless. A [[wikilink]] in a note's text isn't a relationship: edit the text to remove it.".into(),
            json!({ "from": string, "to": string }),
            &["from", "to"],
        ),
        tool(
            "get_backlinks",
            "List the notes that link to a given note.".into(),
            json!({ "name": string }),
            &["name"],
        ),
        tool(
            "create_daily_note",
            "Create the journal entry for a calendar day (ISO date, e.g. \"2026-06-13\"). One DailyNote exists per day; if it already exists this is a no-op that returns the existing note.".into(),
            json!({ "date": string }),
            &["date"],
        ),
        tool(
            "append_to_daily_note",
            format!("Append markdown to the journal entry for a calendar day (ISO date), creating it if needed. In the text, {LINK_SYNTAX} ISO dates (e.g. 2026-06-13) become date references."),
            json!({ "date": string, "markdown": string }),
            &["date", "markdown"],
        ),
        tool(
            "list_structures",
            "List the entity types (structures) available in this knowledge base, then the facets: groups of properties (such as a to-do's status) that an entity of an accepting structure can have.".into(),
            json!({}),
            &[],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::SqlitePool;
    use tonic::Request;

    use super::*;
    use crate::embed::EmbedHandle;
    use crate::proto::entity_service_server::EntityService as _;
    use crate::proto::{
        resolve_entity_request, AddRelationTargetRequest, CreateEntityRequest, ListEntitiesRequest,
        RenameEntityRequest, ResolveEntityRequest,
    };
    use crate::services::search::SearchService;
    use crate::test_support::{entity_service, memory_pool, richtext_service};
    use crate::watch::WatchHub;

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

    #[tokio::test]
    async fn a_note_with_a_link_and_a_tag_derives_two_links_and_backlinks() {
        let f = fixture().await;
        f.k.create_note("Attention", "", None).await.unwrap();
        f.k.create_note("Positional Encoding", "", None)
            .await
            .unwrap();
        let saved =
            f.k.create_note(
                "Transformers",
                "An architecture built on [[Attention]]. #ml",
                None,
            )
            .await
            .unwrap();
        assert!(
            saved.contains("Wikilinks: 2 [") && saved.contains("Note") && saved.contains("Tag"),
            "{saved}"
        );

        assert_eq!(
            f.k.get_backlinks("Attention").await.unwrap(),
            "- Transformers"
        );
        assert_eq!(
            f.k.get_backlinks("Transformers").await.unwrap(),
            "Nothing links to \"Transformers\" yet."
        );
        assert_eq!(
            err(f.k.get_backlinks("Nowhere").await),
            "No note named \"Nowhere\". Use search_notes or create_note."
        );

        let note = f.k.get_note(NoteKey::Name("Transformers")).await.unwrap();
        assert!(
            note.contains("[[Attention]]") && note.contains("#ml"),
            "{note}"
        );

        f.k.append_to_note("Transformers", "They use [[Positional Encoding]] too.")
            .await
            .unwrap();
        let note = f.k.get_note(NoteKey::Name("Transformers")).await.unwrap();
        assert!(
            note.contains("[[Positional Encoding]]") && note.contains("[[Attention]]"),
            "{note}"
        );
        assert_eq!(
            f.k.get_backlinks("Positional Encoding").await.unwrap(),
            "- Transformers"
        );
    }

    #[tokio::test]
    async fn search_finds_notes_by_prefix() {
        let f = fixture().await;
        f.k.create_note("Transformers", "An architecture.", None)
            .await
            .unwrap();
        let found =
            f.k.search_notes("trans", 10, SearchMode::Hybrid)
                .await
                .unwrap();
        assert!(found.contains("[Note] Transformers"), "{found}");
        assert_eq!(
            f.k.search_notes("zzzz", 10, SearchMode::Lexical)
                .await
                .unwrap(),
            "No matches for \"zzzz\"."
        );
    }

    #[tokio::test]
    async fn writing_to_a_missing_note_is_an_error_and_creates_nothing() {
        let f = fixture().await;
        let before = f.entity_count().await;
        for failure in [
            err(f.k.append_to_note("Ghost", "x").await),
            err(f.k.link_notes("Ghost", "Attention").await),
            err(f.k.query_text_location(NoteKey::Name("Ghost"), "x").await),
            err(f.k.edit_note(NoteKey::Name("Ghost"), 1, 1, "x", 0).await),
        ] {
            assert!(failure.contains("No note named"), "{failure}");
        }
        assert_eq!(f.entity_count().await, before);
    }

    #[tokio::test]
    async fn daily_notes_are_one_per_day() {
        let f = fixture().await;
        f.k.create_note("Transformers", "x", None).await.unwrap();
        let created = f.k.create_daily_note("2026-06-13").await.unwrap();
        assert!(
            created.contains("\"June 13, 2026\"") && created.ends_with("for 2026-06-13 created."),
            "{created}"
        );
        let again = f.k.create_daily_note("2026-06-13").await.unwrap();
        assert!(
            again.ends_with("for 2026-06-13 already existed."),
            "{again}"
        );

        f.k.append_to_daily_note("2026-06-13", "Read about [[Transformers]] today.")
            .await
            .unwrap();
        assert!(f
            .k
            .get_backlinks("Transformers")
            .await
            .unwrap()
            .contains("June 13, 2026"));
    }

    #[tokio::test]
    async fn get_note_writes_chips_from_the_targets_current_names() {
        let f = fixture().await;
        f.k.create_note("Old Name", "", None).await.unwrap();
        f.k.create_note("Source", "Points at [[Old Name]].", None)
            .await
            .unwrap();
        let id = f.note_id("Old Name").await;
        let rename = RenameEntityRequest {
            id,
            name: "New Name".into(),
        };
        f.entity.rename_entity(Request::new(rename)).await.unwrap();
        let note = f.k.get_note(NoteKey::Name("Source")).await.unwrap();
        assert!(
            note.contains("[[New Name]]") && !note.contains("Old Name"),
            "{note}"
        );
    }

    #[tokio::test]
    async fn get_note_copied_into_create_note_links_the_same_targets() {
        let f = fixture().await;
        f.create("Tag", "ship").await;
        f.k.create_daily_note("2026-06-13").await.unwrap();
        f.k.create_note("RT Note", "", None).await.unwrap();
        f.k.create_note("RT Alias", "", None).await.unwrap();
        let md = "Uses [[RT Note]] and [[RT Alias|the alias]]. #rt\n\n- do [[Tag/ship]] on [[DailyNote/June 13, 2026]]";
        let tags = ["rt-a".to_string(), "rt-b".to_string()];
        f.k.create_note("Round Trip", md, Some(&tags))
            .await
            .unwrap();
        let id = f.note_id("Round Trip").await;
        let links_before = f.link_targets(&id).await;
        // Five from the body, two from `tags`.
        assert_eq!(links_before.len(), 7, "{links_before:?}");
        assert!(
            links_before.iter().any(|l| l.starts_with("Tag:"))
                && links_before.iter().any(|l| l.starts_with("DailyNote:"))
        );

        let count = f.entity_count().await;
        let note = f.k.get_note(NoteKey::Name("Round Trip")).await.unwrap();
        assert!(
            note.contains("[[Tag/ship]]") && note.contains("[[DailyNote/June 13, 2026]]"),
            "{note}"
        );
        assert!(
            note.contains("[[RT Alias]]") && !note.contains("the alias") && note.contains("#rt"),
            "{note}"
        );
        assert!(
            note.starts_with("---\nname: \"Round Trip\"\nversion: "),
            "{note}"
        );
        let frontmatter = note
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(front, _)| front)
            .unwrap();
        let lines: Vec<&str> = frontmatter.lines().collect();
        assert!(
            lines.len() == 4
                && lines[0].starts_with("name: ")
                && lines[1].starts_with("version: ")
                && lines[2] == "tags: [\"rt-a\",\"rt-b\"]"
                && lines[3] == "related: []",
            "{note}"
        );

        // The whole result, frontmatter included, goes into a new note.
        f.k.create_note("Round Trip Copy", &note, None)
            .await
            .unwrap();
        assert_eq!(
            body(
                &f.k.get_note(NoteKey::Name("Round Trip Copy"))
                    .await
                    .unwrap()
            ),
            body(&note)
        );
        assert_eq!(
            f.link_targets(&f.note_id("Round Trip Copy").await).await,
            links_before
        );
        let source = f.k.entity(&id).await.unwrap();
        let copy =
            f.k.entity(&f.note_id("Round Trip Copy").await)
                .await
                .unwrap();
        assert_eq!(tag_ids(&copy), tag_ids(&source));
        assert_eq!(tag_ids(&copy).len(), 2, "the copy keeps the source's tags");
        assert_eq!(
            f.entity_count().await,
            count + 1,
            "only the copy itself is created"
        );
    }

    #[tokio::test]
    async fn get_note_frontmatter_has_name_version_tags_and_related() {
        let f = fixture().await;
        f.k.create_note("Target", "Body.", None).await.unwrap();
        f.k.create_note("Source", "Points at [[Target]].", None)
            .await
            .unwrap();

        let note = f.k.get_note(NoteKey::Name("Target")).await.unwrap();
        let frontmatter = note
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(front, _)| front)
            .unwrap();
        let keys: Vec<&str> = frontmatter
            .lines()
            .map(|l| l.split_once(':').unwrap().0)
            .collect();
        assert_eq!(keys, ["name", "version", "tags", "related"], "{note}");

        let backlinks = f.k.get_backlinks("Target").await.unwrap();
        assert!(backlinks.contains("Source"), "{backlinks}");
    }

    #[tokio::test]
    async fn create_note_refuses_an_existing_note() {
        let f = fixture().await;
        f.k.create_note("Existing", "Original.", None)
            .await
            .unwrap();
        let refused = err(f.k.create_note("Existing", "Replacement.", None).await);
        assert!(refused.contains("already exists"), "{refused}");
        assert!(
            err(f.k.create_note("existing", "Case.", None).await).contains("already exists"),
            "names match case-insensitively"
        );
        assert_eq!(
            body(&f.k.get_note(NoteKey::Name("Existing")).await.unwrap()),
            "Original."
        );

        f.k.create_note("Blank", "", None).await.unwrap();
        assert!(
            err(f.k.create_note("Blank", "Filled.", None).await).contains("already exists"),
            "even a blank note exists"
        );
    }

    // I-61: `[[Name|Alias]]` and `]]` can't be told from a name containing them.
    #[tokio::test]
    async fn create_note_refuses_a_pipe_in_the_name() {
        let f = fixture().await;
        let before = f.entity_count().await;

        for name in ["A|B", "x]]y"] {
            let refused = err(f.k.create_note(name, "Body.", None).await);
            assert_eq!(refused, r#"names can't contain "|" or "]]""#);
        }
        assert_eq!(f.entity_count().await, before, "no note was created");
        assert!(f.notes_named("A|B").await.is_empty());

        // A lone bracket is a fine name.
        f.k.create_note("a[b]c", "Body.", None).await.unwrap();
        assert_eq!(f.notes_named("a[b]c").await.len(), 1);
    }

    // The name is refused before resolving the body's #tags and the `tags` list,
    // which would create Tags and leave them behind.
    #[tokio::test]
    async fn create_note_with_a_refused_name_leaves_nothing_behind() {
        let f = fixture().await;
        let before = f.entity_count().await;

        let refused = err(f
            .k
            .create_note("A|B", "Body #leaky.", Some(&["t1".into()]))
            .await);

        assert_eq!(refused, r#"names can't contain "|" or "]]""#);
        assert_eq!(f.entity_count().await, before, "no note or Tag was created");
    }

    // A note's name can look like a structure path: `[[Note/Tag/X]]` is the Note
    // named "Tag/X", never the Tag "X", and link_notes takes the name as it is.
    #[tokio::test]
    async fn a_note_named_like_a_structure_path_round_trips() {
        let f = fixture().await;
        f.create("Tag", "X").await;
        f.k.create_note("Tag/X", "The note.", None).await.unwrap();
        let target = f.note_id("Tag/X").await;
        f.k.create_note("Source", "See [[Note/Tag/X]].", None)
            .await
            .unwrap();
        f.k.create_note("Third", "", None).await.unwrap();

        let source = f.k.get_note(NoteKey::Name("Source")).await.unwrap();
        assert_eq!(body(&source), "See [[Note/Tag/X]].", "{source}");
        assert_eq!(
            f.link_targets(&f.note_id("Source").await).await,
            [format!("Note:{target}")]
        );

        assert_eq!(
            f.k.link_notes("Third", "Tag/X").await.unwrap(),
            "Related \"Third\" -> \"Tag/X\"."
        );
        assert_eq!(
            f.link_targets(&f.note_id("Third").await).await,
            [format!("Note:{target}")]
        );
    }

    #[tokio::test]
    async fn a_link_to_a_missing_target_fails_the_write_and_creates_nothing() {
        let f = fixture().await;
        f.k.create_note("Existing", "Body.", None).await.unwrap();
        let before = f.entity_count().await;
        let md = "See [[Attenton]] and [[DailyNote/February 1, 1901]].";
        for failure in [
            err(f.k.create_note("New", md, None).await),
            err(f.k.append_to_note("Existing", md).await),
            err(f.k.append_to_daily_note("2026-06-13", md).await),
            err(f
                .k
                .edit_note(
                    NoteKey::Name("Existing"),
                    1,
                    1,
                    md,
                    version(&f.k.get_note(NoteKey::Name("Existing")).await.unwrap()),
                )
                .await),
        ] {
            assert!(
                failure.contains("Nothing was written: 2 link(s)"),
                "{failure}"
            );
            assert!(
                failure.contains("[[Attenton]], [[DailyNote/February 1, 1901]]"),
                "{failure}"
            );
        }
        assert_eq!(
            f.entity_count().await,
            before,
            "not even append_to_daily_note's day"
        );
        assert_eq!(
            body(&f.k.get_note(NoteKey::Name("Existing")).await.unwrap()),
            "Body."
        );
        assert!(err(f.k.get_note(NoteKey::Name("New")).await).starts_with("No note named"));
    }

    #[tokio::test]
    async fn a_missing_tag_is_created() {
        let f = fixture().await;
        let saved =
            f.k.create_note("Tagged", "About #brandnew things.", None)
                .await
                .unwrap();
        assert!(saved.contains("Wikilinks: 1 [Tag]"), "{saved}");
    }

    #[tokio::test]
    async fn create_note_may_link_to_itself() {
        let f = fixture().await;
        f.k.create_note("Selfie", "I am [[Selfie]].", None)
            .await
            .unwrap();
        let id = f.note_id("Selfie").await;
        assert_eq!(
            body(&f.k.get_note(NoteKey::Name("Selfie")).await.unwrap()),
            "I am [[Selfie]]."
        );
        assert_eq!(f.link_targets(&id).await, [format!("Note:{id}")]);
    }

    #[tokio::test]
    async fn a_leading_rule_that_is_not_frontmatter_is_note_content() {
        let f = fixture().await;
        f.k.create_note("Rule", "---\nplain text\n---", None)
            .await
            .unwrap();
        assert!(body(&f.k.get_note(NoteKey::Name("Rule")).await.unwrap()).contains("plain text"));
    }

    #[tokio::test]
    async fn query_text_location_reports_whole_blocks_and_the_version() {
        let f = fixture().await;
        f.k.create_note("Link", "", None).await.unwrap();
        f.k.create_note(
            "Subject",
            "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]].",
            None,
        )
        .await
        .unwrap();
        let note = f.k.get_note(NoteKey::Name("Subject")).await.unwrap();
        assert_eq!(
            body(&note),
            "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]]."
        );
        let v = version(&note);

        let found =
            f.k.query_text_location(NoteKey::Name("Subject"), "two")
                .await
                .unwrap();
        assert_eq!(
            found,
            format!("version: {v}\n\nlines 5-6 (1 match):\n- one\n- two")
        );
        let found =
            f.k.query_text_location(NoteKey::Name("Subject"), "Alpha.\n\n## Heading")
                .await
                .unwrap();
        assert!(
            found.contains("lines 1-1 (1 match):\nAlpha.\n\nlines 3-3 (1 match):\n## Heading"),
            "{found}"
        );
        let found =
            f.k.query_text_location(NoteKey::Name("Subject"), "o")
                .await
                .unwrap();
        assert!(
            found.ends_with("lines 5-6 (2 matches):\n- one\n- two"),
            "{found}"
        );
        let found =
            f.k.query_text_location(NoteKey::Name("Subject"), "n")
                .await
                .unwrap();
        let (a, b, c) = (
            found.find("lines 3-3").unwrap(),
            found.find("lines 5-6").unwrap(),
            found.find("lines 8-8").unwrap(),
        );
        assert!(a < b && b < c, "{found}");
        assert!(f
            .k
            .query_text_location(NoteKey::Name("Subject"), "nowhere in the note")
            .await
            .unwrap()
            .starts_with("No match"));
        assert!(
            err(f.k.query_text_location(NoteKey::Name("Subject"), "").await)
                .contains("query is empty")
        );
    }

    #[tokio::test]
    async fn edit_note_replaces_inserts_and_deletes_whole_blocks() {
        let f = fixture().await;
        f.k.create_note("Link", "", None).await.unwrap();
        f.k.create_note(
            "Subject",
            "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]].",
            None,
        )
        .await
        .unwrap();
        let id = f.note_id("Subject").await;
        let read = || async {
            let note = f.k.get_note(NoteKey::Name("Subject")).await.unwrap();
            (body(&note).to_string(), version(&note))
        };

        let (_, v) = read().await;
        f.k.edit_note(NoteKey::Name("Subject"), 3, 3, "## Renamed heading", v)
            .await
            .unwrap();
        let (text, v) = read().await;
        assert_eq!(
            text,
            "Alpha.\n\n## Renamed heading\n\n- one\n- two\n\nLast [[Link]]."
        );

        let stale = err(f
            .k
            .edit_note(NoteKey::Name("Subject"), 1, 1, "Stale", v - 1)
            .await);
        assert!(stale.contains("changed since you read"), "{stale}");
        assert!(!read().await.0.contains("Stale"));

        let inside_start = err(f.k.edit_note(NoteKey::Name("Subject"), 6, 6, "x", v).await);
        assert!(
            inside_start.contains("line 6 is inside the block at lines 5-6")
                && inside_start.contains("start at 5"),
            "{inside_start}"
        );
        let inside_end = err(f.k.edit_note(NoteKey::Name("Subject"), 5, 5, "x", v).await);
        assert!(inside_end.contains("end at 6"), "{inside_end}");
        assert!(
            err(f.k.edit_note(NoteKey::Name("Subject"), 2, 2, "x", v).await)
                .contains("contain no block")
        );
        assert_eq!(read().await.0, text, "a rejected range writes nothing");

        f.k.edit_note(NoteKey::Name("Subject"), 1, 0, "First.", v)
            .await
            .unwrap();
        let (text, v) = read().await;
        assert!(
            text.starts_with("First.\n\nAlpha.\n\n## Renamed heading"),
            "{text}"
        );

        // Blank lines at either edge of a range are fine.
        f.k.edit_note(NoteKey::Name("Subject"), 2, 4, "Alpha two.", v)
            .await
            .unwrap();
        let (text, v) = read().await;
        assert!(
            text.starts_with("First.\n\nAlpha two.\n\n## Renamed heading"),
            "{text}"
        );

        // Now: First. 1, Alpha two. 3, heading 5, list 7-8, Last 10.
        assert_eq!(f.link_targets(&id).await.len(), 1);
        f.k.edit_note(NoteKey::Name("Subject"), 10, 10, "Last, rewritten.", v)
            .await
            .unwrap();
        assert!(
            f.link_targets(&id).await.is_empty(),
            "replacing the block that held a link drops the link"
        );

        let (_, v) = read().await;
        f.k.edit_note(NoteKey::Name("Subject"), 5, 8, "", v)
            .await
            .unwrap();
        let (text, v) = read().await;
        assert_eq!(text, "First.\n\nAlpha two.\n\nLast, rewritten.");

        f.k.edit_note(NoteKey::Name("Subject"), 6, 5, "Appended.", v)
            .await
            .unwrap();
        let (text, v) = read().await;
        assert_eq!(
            text,
            "First.\n\nAlpha two.\n\nLast, rewritten.\n\nAppended."
        );

        assert!(err(f
            .k
            .edit_note(NoteKey::Name("Subject"), 99, 99, "x", v)
            .await)
        .contains("start 99 is outside"));
        assert!(
            err(f.k.edit_note(NoteKey::Name("Subject"), 1, 99, "x", v).await)
                .contains("end 99 is outside")
        );
        assert!(
            err(f.k.edit_note(NoteKey::Name("Subject"), 0, 0, "x", v).await)
                .contains("start must be at least 1")
        );
        assert!(
            err(f.k.edit_note(NoteKey::Name("Subject"), 1, 1, "x", -1).await)
                .contains("version at least 0")
        );
        assert!(
            err(f.k.edit_note(NoteKey::Name("Subject"), 3, 1, "x", v).await)
                .contains("end 1 is outside")
        );

        f.k.edit_note(NoteKey::Name("Subject"), 1, 7, "", v)
            .await
            .unwrap();
        let (text, v) = read().await;
        assert_eq!(text, "", "deleting every block leaves an empty body");
        f.k.edit_note(NoteKey::Name("Subject"), 1, 0, "Fresh.", v)
            .await
            .unwrap();
        assert_eq!(read().await.0, "Fresh.");
        let (doc, _) =
            f.k.get_doc(&tools::doc_ref("Note", &id).unwrap())
                .await
                .unwrap();
        assert_eq!(
            doc["content"].as_array().unwrap().len(),
            1,
            "no stray empty paragraph"
        );
    }

    #[tokio::test]
    async fn append_keeps_nodes_the_converter_does_not_know() {
        let f = fixture().await;
        f.k.create_note("Rich", "First.", None).await.unwrap();
        let id = f.note_id("Rich").await;
        let r = tools::doc_ref("Note", &id).unwrap();
        let (_, updated_at) = f.k.get_doc(&r).await.unwrap();
        let doc = json!({ "type": "doc", "content": [
            { "type": "paragraph", "content": [{ "type": "text", "text": "kept", "marks": [{ "type": "link", "attrs": { "href": "u" } }] }] },
            { "type": "horizontalRule" },
        ]});
        f.k.put_doc(&r, &doc, Some(updated_at)).await.unwrap();

        f.k.append_to_note("Rich", "Second.").await.unwrap();
        let (saved, _) = f.k.get_doc(&r).await.unwrap();
        assert_eq!(saved["content"][0], doc["content"][0]);
        assert_eq!(saved["content"][1], doc["content"][1]);
        assert_eq!(saved["content"][2]["content"][0]["text"], "Second.");
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

    #[tokio::test]
    async fn edit_note_keeps_an_image_it_inserts_beside() {
        let f = fixture().await;
        let image = json!({ "type": "image", "attrs": { "src": "data:image/png;base64,AAAA", "width": "40%" } });
        let r = note_with_blocks(&f, "Picture", json!([image])).await;
        let note = f.k.get_note(NoteKey::Name("Picture")).await.unwrap();
        assert_eq!(body(&note), "<!-- calcifer:image -->");

        f.k.edit_note(
            NoteKey::Name("Picture"),
            1,
            0,
            "Caption first.",
            version(&note),
        )
        .await
        .unwrap();
        let (doc, _) = f.k.get_doc(&r).await.unwrap();
        assert_eq!(doc["content"][0]["content"][0]["text"], "Caption first.");
        assert_eq!(doc["content"][1], image, "the image survives the insert");
    }

    #[tokio::test]
    async fn edit_note_refuses_a_range_over_what_markdown_cannot_express() {
        let f = fixture().await;
        let table = json!({ "type": "table", "content": [] });
        let para = |text: &str| json!({ "type": "paragraph", "content": [{ "type": "text", "text": text }] });
        let r = note_with_blocks(
            &f,
            "Tabular",
            json!([para("Intro."), table, para("Outro.")]),
        )
        .await;
        let note = f.k.get_note(NoteKey::Name("Tabular")).await.unwrap();
        assert_eq!(body(&note), "Intro.\n\n<!-- calcifer:table -->\n\nOutro.");
        let (before, _) = f.k.get_doc(&r).await.unwrap();

        let refused = err(f
            .k
            .edit_note(
                NoteKey::Name("Tabular"),
                1,
                5,
                "Everything, rewritten.",
                version(&note),
            )
            .await);
        assert!(
            refused.contains(
                "lines 3-3 hold a table that markdown can't express; leave them out of the range"
            ),
            "{refused}"
        );
        assert_eq!(
            f.k.get_doc(&r).await.unwrap().0,
            before,
            "a refused edit writes nothing"
        );

        // Ranges beside it are fine.
        f.k.edit_note(
            NoteKey::Name("Tabular"),
            5,
            5,
            "Outro, rewritten.",
            version(&note),
        )
        .await
        .unwrap();
        let (after, _) = f.k.get_doc(&r).await.unwrap();
        assert_eq!(after["content"][1], table);
    }

    #[tokio::test]
    async fn edit_note_refuses_formatting_markdown_cannot_express() {
        let f = fixture().await;
        let link = json!({ "type": "link", "attrs": {
            "href": "https://example.com", "target": "_blank", "rel": "noopener noreferrer", "class": null, "title": null,
        } });
        let r = note_with_blocks(&f, "Styled", json!([
            { "type": "paragraph", "attrs": { "textAlign": null }, "content": [
                { "type": "text", "text": "An " },
                { "type": "text", "text": "underlined", "marks": [{ "type": "underline" }] },
                { "type": "text", "text": " word." },
            ] },
            { "type": "heading", "attrs": { "textAlign": "center", "level": 2 }, "content": [{ "type": "text", "text": "Centred" }] },
            { "type": "paragraph", "attrs": { "textAlign": null }, "content": [
                { "type": "text", "text": "See " },
                { "type": "text", "text": "the site", "marks": [link] },
                { "type": "text", "text": "." },
            ] },
        ])).await;
        let note = f.k.get_note(NoteKey::Name("Styled")).await.unwrap();
        assert_eq!(
            body(&note),
            "An underlined word.\n\n## Centred\n\nSee [the site](https://example.com)."
        );
        let (before, _) = f.k.get_doc(&r).await.unwrap();

        let underline = err(f
            .k
            .edit_note(NoteKey::Name("Styled"), 1, 1, "Rewritten.", version(&note))
            .await);
        assert!(
            underline.contains("lines 1-1 hold underline formatting that markdown can't express"),
            "{underline}"
        );
        let centred = err(f
            .k
            .edit_note(
                NoteKey::Name("Styled"),
                3,
                3,
                "## Rewritten",
                version(&note),
            )
            .await);
        assert!(
            centred.contains("lines 3-3 hold a heading's textAlign that markdown can't express"),
            "{centred}"
        );
        assert_eq!(
            f.k.get_doc(&r).await.unwrap().0,
            before,
            "a refused edit writes nothing"
        );

        f.k.edit_note(
            NoteKey::Name("Styled"),
            5,
            5,
            "Now see [the docs](https://docs.rs/x_(y)).",
            version(&note),
        )
        .await
        .unwrap();
        let (after, _) = f.k.get_doc(&r).await.unwrap();
        let linked = &after["content"][2]["content"][1];
        assert_eq!(linked["text"], "the docs");
        assert_eq!(linked["marks"][0]["type"], "link");
        assert_eq!(linked["marks"][0]["attrs"]["href"], "https://docs.rs/x_(y)");
        assert_eq!(
            after["content"][0], before["content"][0],
            "the blocks before it are untouched"
        );
        assert_eq!(after["content"][1], before["content"][1]);
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

    #[tokio::test]
    async fn get_note_by_date_reads_the_daily_note() {
        let f = fixture().await;
        f.k.append_to_daily_note("2026-10-24", "Met the team.\n\nShipped it.")
            .await
            .unwrap();

        let note = f.k.get_note(NoteKey::Date("2026-10-24")).await.unwrap();
        assert_eq!(
            frontmatter_keys(&note),
            ["name", "version", "date"],
            "{note}"
        );
        assert!(
            note.starts_with("---\nname: \"October 24, 2026\"\nversion: "),
            "{note}"
        );
        assert!(note.contains("\ndate: \"2026-10-24\"\n"), "{note}");
        assert!(
            !note.contains("tags") && !note.contains("related"),
            "{note}"
        );
        assert!(version(&note) > 0, "{note}");
        assert_eq!(body(&note), "Met the team.\n\nShipped it.");
    }

    #[tokio::test]
    async fn get_note_by_missing_date_creates_nothing() {
        let f = fixture().await;
        let before = f.entity_count().await;

        let failure = err(f.k.get_note(NoteKey::Date("2026-10-24")).await);
        assert_eq!(
            failure,
            "No daily note for 2026-10-24. Use append_to_daily_note to start one."
        );
        for failure in [
            err(f
                .k
                .query_text_location(NoteKey::Date("2026-10-24"), "x")
                .await),
            err(f
                .k
                .edit_note(NoteKey::Date("2026-10-24"), 1, 0, "x", 0)
                .await),
        ] {
            assert!(failure.contains("append_to_daily_note"), "{failure}");
        }

        assert_eq!(f.entity_count().await, before);
        let req = ResolveEntityRequest {
            structure_type: String::new(),
            key: Some(resolve_entity_request::Key::Date("2026-10-24".into())),
            create_if_missing: false,
        };
        let lookup = f.entity.resolve_entity(Request::new(req)).await;
        assert_eq!(lookup.unwrap_err().code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn get_note_missing_note_is_an_error() {
        let f = fixture().await;
        let before = f.entity_count().await;
        let call = |arguments: Value| {
            handle_message(
                &f.k,
                json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "get_note", "arguments": arguments } }),
            )
        };

        for (arguments, expected) in [
            (
                json!({ "name": "Ghost" }),
                "No note named \"Ghost\". Use search_notes or create_note.",
            ),
            (
                json!({ "date": "2026-10-24" }),
                "No daily note for 2026-10-24. Use append_to_daily_note to start one.",
            ),
        ] {
            let reply = call(arguments.clone()).await.unwrap();
            assert!(reply.get("error").is_none(), "{arguments}: {reply}");
            assert_eq!(reply["result"]["isError"], true, "{arguments}: {reply}");
            assert_eq!(
                reply["result"]["content"][0]["text"], expected,
                "{arguments}"
            );
        }
        assert_eq!(f.entity_count().await, before, "nothing was created");
    }

    #[tokio::test]
    async fn get_backlinks_missing_note_is_an_error() {
        let f = fixture().await;
        let before = f.entity_count().await;

        let reply = handle_message(
            &f.k,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "get_backlinks", "arguments": { "name": "Ghost" } } }),
        )
        .await
        .unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        assert_eq!(
            reply["result"]["content"][0]["text"],
            "No note named \"Ghost\". Use search_notes or create_note."
        );
        assert_eq!(f.entity_count().await, before, "nothing was created");
    }

    #[tokio::test]
    async fn a_malformed_date_fails_with_the_servers_message() {
        let f = fixture().await;
        let before = f.entity_count().await;

        for failure in [
            err(f.k.get_note(NoteKey::Date("tomorrow")).await),
            err(f
                .k
                .query_text_location(NoteKey::Date("tomorrow"), "x")
                .await),
            err(f.k.edit_note(NoteKey::Date("tomorrow"), 1, 0, "x", 0).await),
        ] {
            assert!(
                failure.contains("date must be an ISO calendar day (yyyy-MM-dd)"),
                "{failure}"
            );
        }
        assert_eq!(f.entity_count().await, before);
    }

    #[tokio::test]
    async fn query_and_edit_a_daily_note_by_date() {
        let f = fixture().await;
        let day = NoteKey::Date;
        f.k.append_to_daily_note("2026-10-24", "First.\n\nSecond.\n\nThird.")
            .await
            .unwrap();

        let found =
            f.k.query_text_location(day("2026-10-24"), "Second")
                .await
                .unwrap();
        let v = version(&found);
        assert!(found.contains("lines 3-3 (1 match):\nSecond."), "{found}");
        assert!(f
            .k
            .query_text_location(day("2026-10-24"), "nowhere")
            .await
            .unwrap()
            .ends_with("in Daily note \"October 24, 2026\"."));

        let done =
            f.k.edit_note(day("2026-10-24"), 3, 3, "Replaced.", v)
                .await
                .unwrap();
        assert!(
            done.starts_with("Replaced lines 3-3 of Daily note \"October 24, 2026\" ("),
            "{done}"
        );
        let note = f.k.get_note(day("2026-10-24")).await.unwrap();
        assert_eq!(body(&note), "First.\n\nReplaced.\n\nThird.");
        assert_eq!(f.entity_count().await, 1, "still the one day");
    }

    #[tokio::test]
    async fn edit_note_by_date_refuses_a_stale_version() {
        let f = fixture().await;
        f.k.append_to_daily_note("2026-10-24", "First.")
            .await
            .unwrap();
        f.k.create_note("Subject", "First.", None).await.unwrap();
        let day = f.k.get_note(NoteKey::Date("2026-10-24")).await.unwrap();
        let note = f.k.get_note(NoteKey::Name("Subject")).await.unwrap();

        let stale = err(f
            .k
            .edit_note(
                NoteKey::Date("2026-10-24"),
                1,
                1,
                "Stale.",
                version(&day) - 1,
            )
            .await);
        assert!(
            stale.starts_with("Daily note \"October 24, 2026\" changed since you read it."),
            "{stale}"
        );
        let stale = err(f
            .k
            .edit_note(NoteKey::Name("Subject"), 1, 1, "Stale.", version(&note) - 1)
            .await);
        assert!(
            stale.starts_with("Note \"Subject\" changed since you read it."),
            "{stale}"
        );
        let day = f.k.get_note(NoteKey::Date("2026-10-24")).await.unwrap();
        assert_eq!(body(&day), "First.", "a refused edit writes nothing");
    }

    #[tokio::test]
    async fn note_key_needs_exactly_one_of_name_and_date() {
        let f = fixture().await;
        f.k.create_note("A", "Body.", None).await.unwrap();
        let call = |name: &str, arguments: Value| {
            handle_message(
                &f.k,
                json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
            )
        };
        let rest = |tool: &str| match tool {
            "get_note" => json!({}),
            "query_text_location" => json!({ "query": "x" }),
            _ => json!({ "start": 1, "end": 1, "markdown": "x", "version": 0 }),
        };

        for tool in ["get_note", "query_text_location", "edit_note"] {
            for key in [json!({ "name": "A", "date": "2026-10-24" }), json!({})] {
                let mut args = rest(tool);
                args.as_object_mut()
                    .unwrap()
                    .extend(key.as_object().unwrap().clone());
                let reply = call(tool, args).await.unwrap();
                assert!(reply.get("error").is_none(), "{tool}: {reply}");
                assert_eq!(reply["result"]["isError"], true, "{tool}: {reply}");
                let text = reply["result"]["content"][0]["text"].as_str().unwrap();
                assert!(
                    text.starts_with("Give either name (a note) or date (a daily note"),
                    "{tool}: {text}"
                );
            }
        }
        assert_eq!(f.entity_count().await, 1, "nothing was created");
    }

    #[test]
    fn tools_that_take_a_note_or_a_day_declare_date_and_require_no_name() {
        let defs = tool_definitions();
        for tool in ["get_note", "query_text_location", "edit_note"] {
            let def = defs.iter().find(|t| t["name"] == tool).unwrap();
            let schema = &def["inputSchema"];
            assert_eq!(
                schema["properties"]["date"],
                json!({ "type": "string" }),
                "{tool}"
            );
            assert_eq!(
                schema["properties"]["name"],
                json!({ "type": "string" }),
                "{tool}"
            );
            let required = schema["required"].as_array().unwrap();
            assert!(!required.contains(&json!("name")), "{tool}: {required:?}");
            assert!(!required.contains(&json!("date")), "{tool}: {required:?}");
        }
    }

    #[tokio::test]
    async fn list_structures_describes_the_registry() {
        let f = fixture().await;
        let listed = f.k.list_structures();
        assert!(
            listed.contains("- Note: ") && listed.contains("- DailyNote: "),
            "{listed}"
        );
        assert!(listed.contains("(relations -> Tag)"), "{listed}");
    }

    #[tokio::test]
    async fn link_notes_relates_without_writing_text() {
        let f = fixture().await;
        f.k.create_note("A", "Body A.", None).await.unwrap();
        f.k.create_note("B", "Body B.", None).await.unwrap();
        let before = f.k.get_note(NoteKey::Name("A")).await.unwrap();

        assert_eq!(
            f.k.link_notes("A", "B").await.unwrap(),
            "Related \"A\" -> \"B\"."
        );
        let after = f.k.get_note(NoteKey::Name("A")).await.unwrap();
        assert_eq!(body(&after), "Body A.", "{after}");
        assert_eq!(version(&after), version(&before), "no text was saved");
        assert!(
            after.contains("\ntags: []\nrelated: [\"B\"]\n---\n"),
            "{after}"
        );
        let b = f.k.get_note(NoteKey::Name("B")).await.unwrap();
        assert!(b.contains("\nrelated: []\n"), "one-way: {b}");

        // Names match case-insensitively; the message uses the stored names.
        assert_eq!(
            f.k.link_notes("a", "b").await.unwrap(),
            "\"A\" was already related to \"B\"."
        );
        let again = f.k.get_note(NoteKey::Name("A")).await.unwrap();
        assert_eq!(body(&again), "Body A.");
        assert!(again.contains("\nrelated: [\"B\"]\n"), "{again}");
    }

    #[tokio::test]
    async fn get_note_lists_related_oldest_first() {
        let f = fixture().await;
        for name in ["A", "B", "C"] {
            f.k.create_note(name, "", None).await.unwrap();
        }
        f.k.link_notes("A", "C").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        f.k.link_notes("A", "B").await.unwrap();

        let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
        assert!(note.contains("\nrelated: [\"C\",\"B\"]\n"), "{note}");

        f.k.unlink_notes("A", "C").await.unwrap();
        let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
        assert!(note.contains("\nrelated: [\"B\"]\n"), "{note}");
    }

    #[tokio::test]
    async fn unlink_notes_removes_the_relationship() {
        let f = fixture().await;
        f.k.create_note("A", "Body A.", None).await.unwrap();
        f.k.create_note("B", "Body B.", None).await.unwrap();
        f.k.link_notes("A", "B").await.unwrap();

        assert_eq!(
            f.k.unlink_notes("A", "B").await.unwrap(),
            "Unrelated \"A\" -> \"B\"."
        );
        let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
        assert!(note.contains("\nrelated: []\n"), "{note}");
        assert_eq!(body(&note), "Body A.");
        assert!(f.link_targets(&f.note_id("A").await).await.is_empty());

        assert_eq!(
            f.k.unlink_notes("A", "B").await.unwrap(),
            "\"A\" wasn't related to \"B\"."
        );
        assert_eq!(
            f.k.unlink_notes("b", "a").await.unwrap(),
            "\"B\" wasn't related to \"A\".",
            "the other direction was never related"
        );
    }

    #[tokio::test]
    async fn link_notes_takes_an_id_and_refuses_an_ambiguous_name() {
        let f = fixture().await;
        f.k.create_note("A", "", None).await.unwrap();
        f.create("Note", "Dup").await;
        f.create("Note", "Dup").await;
        let dups = f.notes_named("Dup").await;
        assert_eq!(dups.len(), 2);

        for failure in [
            err(f.k.link_notes("A", "dup").await),
            err(f.k.link_notes("Dup", "A").await),
            err(f.k.unlink_notes("A", "Dup").await),
        ] {
            assert!(failure.starts_with("Several notes are named"), "{failure}");
            for id in &dups {
                assert!(failure.contains(&format!("- Dup ({id})")), "{failure}");
            }
            assert!(failure.ends_with("Call again with the id."), "{failure}");
        }
        assert!(f.link_targets(&f.note_id("A").await).await.is_empty());

        assert_eq!(
            f.k.link_notes("A", &dups[0]).await.unwrap(),
            "Related \"A\" -> \"Dup\"."
        );
        assert_eq!(
            f.link_targets(&f.note_id("A").await).await,
            [format!("Note:{}", dups[0])]
        );
        assert_eq!(
            f.k.link_notes(&dups[1], "A").await.unwrap(),
            "Related \"Dup\" -> \"A\"."
        );
        assert_eq!(
            f.k.unlink_notes("A", &dups[0]).await.unwrap(),
            "Unrelated \"A\" -> \"Dup\"."
        );
    }

    #[tokio::test]
    async fn link_notes_missing_note_is_an_error() {
        let f = fixture().await;
        f.k.create_note("A", "", None).await.unwrap();
        f.create("Tag", "ml").await;
        let tag = tag_id(&f, "ml").await;
        let before = f.entity_count().await;

        for failure in [
            err(f.k.link_notes("A", "Ghost").await),
            err(f.k.link_notes("Ghost", "A").await),
            err(f.k.unlink_notes("A", "Ghost").await),
            err(f.k.unlink_notes("Ghost", "A").await),
            err(f.k.link_notes("A", &uuid::Uuid::new_v4().to_string()).await),
            err(f.k.link_notes("A", "[[A]]").await),
        ] {
            assert!(
                failure.starts_with("No note named or with id \"")
                    && failure.ends_with("\". Use search_notes."),
                "{failure}"
            );
        }
        assert!(
            err(f.k.link_notes("A", &tag).await).starts_with("No note named or with id"),
            "the id of a Tag isn't a note's"
        );
        assert!(
            err(f.k.link_notes("A", "ml").await).starts_with("No note named or with id"),
            "a Tag's name isn't a note's"
        );
        assert_eq!(f.entity_count().await, before, "nothing is created");
        let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
        assert!(note.contains("\nrelated: []\n"), "{note}");
    }

    #[tokio::test]
    async fn link_notes_refuses_a_note_related_to_itself() {
        let f = fixture().await;
        f.k.create_note("A", "", None).await.unwrap();
        let failure = err(f.k.link_notes("A", "a").await);
        assert!(failure.contains("itself"), "{failure}");
    }

    #[tokio::test]
    async fn get_backlinks_labels_related_sources() {
        let f = fixture().await;
        f.k.create_note("B", "", None).await.unwrap();
        f.k.create_note("A", "", None).await.unwrap();
        f.k.create_note("Mentioner", "Points at [[B]].", None)
            .await
            .unwrap();
        f.k.link_notes("A", "B").await.unwrap();

        let backlinks = f.k.get_backlinks("B").await.unwrap();
        let mut lines: Vec<&str> = backlinks.lines().collect();
        lines.sort_unstable();
        assert_eq!(lines, ["- A (related)", "- Mentioner"], "{backlinks}");

        // A mention and a relationship from one source is one line.
        f.k.append_to_note("A", "Also [[B]].").await.unwrap();
        let backlinks = f.k.get_backlinks("B").await.unwrap();
        assert_eq!(
            backlinks.lines().filter(|l| l.starts_with("- A")).count(),
            1,
            "{backlinks}"
        );
        assert!(backlinks.contains("- A (related)"), "{backlinks}");

        f.k.unlink_notes("A", "B").await.unwrap();
        let backlinks = f.k.get_backlinks("B").await.unwrap();
        assert!(!backlinks.contains("related"), "{backlinks}");
        assert!(
            backlinks.contains("- A\n") || backlinks.ends_with("- A"),
            "{backlinks}"
        );
    }

    #[tokio::test]
    async fn create_note_ignores_frontmatter_related() {
        let f = fixture().await;
        f.k.create_note("A", "Body.", None).await.unwrap();
        f.k.create_note("B", "", None).await.unwrap();
        f.k.link_notes("A", "B").await.unwrap();
        let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
        assert!(note.contains("\nrelated: [\"B\"]\n"), "{note}");

        f.k.create_note("A Copy", &note, None).await.unwrap();
        let copy = f.k.get_note(NoteKey::Name("A Copy")).await.unwrap();
        assert_eq!(body(&copy), "Body.", "{copy}");
        assert!(copy.contains("\nrelated: []\n"), "{copy}");
        assert!(f.link_targets(&f.note_id("A Copy").await).await.is_empty());
        assert_eq!(
            f.link_targets(&f.note_id("A").await).await.len(),
            1,
            "the original keeps its relationship"
        );
    }

    #[tokio::test]
    async fn list_structures_shows_relates_to() {
        let f = fixture().await;
        let listed = f.k.list_structures();
        let entries: Vec<&str> = listed.split("\n- ").collect();
        let entry = |prefix: &str| {
            entries
                .iter()
                .find(|e| e.trim_start_matches("- ").starts_with(prefix))
                .unwrap_or_else(|| panic!("no {prefix} in {listed}"))
        };
        let note = entry("Note: ");
        assert!(note.lines().any(|l| l == "  relates_to: Note"), "{listed}");
        assert!(
            note.find("  facets: ") < note.find("  relates_to: "),
            "relates_to comes after facets: {listed}"
        );
        for other in ["Tag: ", "DailyNote: ", "facet todo "] {
            assert!(!entry(other).contains("relates_to"), "{listed}");
        }
    }

    #[tokio::test]
    async fn list_structures_shows_facets() {
        let f = fixture().await;
        let listed = f.k.list_structures();
        let note = &listed[listed.find("- Note: ").unwrap()..];
        assert!(
            note.lines().take(3).any(|l| l == "  facets: todo"),
            "{listed}"
        );
        let facet = &listed[listed.find("- facet todo (To-do): ").unwrap()..];
        assert!(
            facet.contains("todo.status (select: open|done, default open)"),
            "{listed}"
        );
        assert!(facet.contains("todo.due (date)"), "{listed}");
        assert!(facet.contains("\n  accepted by: Note"), "{listed}");
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

    #[tokio::test]
    async fn create_note_with_tags_sets_the_tags_property() {
        let f = fixture().await;
        f.create("Tag", "ml").await;
        let tags = ["ml".to_string(), "brandnew".to_string(), "ML".to_string()];
        let saved =
            f.k.create_note("Tagged", "Body.", Some(&tags))
                .await
                .unwrap();
        assert!(
            saved.contains("Wikilinks: 0 [none]") && saved.contains("tags: 2."),
            "{saved}"
        );

        let note = f.k.entity(&f.note_id("Tagged").await).await.unwrap();
        let ml = tag_id(&f, "ml").await;
        let brandnew = tag_id(&f, "brandnew").await;
        assert_eq!(
            tag_ids(&note),
            [ml.clone(), brandnew.clone()],
            "a missing Tag is created"
        );
        let mut expected = [format!("Tag:{ml}"), format!("Tag:{brandnew}")];
        expected.sort();
        assert_eq!(f.link_targets(&note.id).await, expected);
    }

    #[tokio::test]
    async fn get_note_frontmatter_shows_tags() {
        let f = fixture().await;
        let tags = ["ml".to_string(), "papers".to_string()];
        f.k.create_note("Tagged", "Body.", Some(&tags))
            .await
            .unwrap();
        f.k.create_note("Plain", "Body #ml.", None).await.unwrap();

        let tagged = f.k.get_note(NoteKey::Name("Tagged")).await.unwrap();
        assert!(
            tagged.contains("\ntags: [\"ml\",\"papers\"]\nrelated: []\n---\n"),
            "{tagged}"
        );
        let plain = f.k.get_note(NoteKey::Name("Plain")).await.unwrap();
        assert!(
            plain.contains("\ntags: []\nrelated: []\n---\n"),
            "a body #tag is not one of the note's tags: {plain}"
        );
    }

    #[tokio::test]
    async fn create_note_tags_argument_wins_over_frontmatter() {
        let f = fixture().await;
        let before = f.entity_count().await;
        let md = "---\nname: \"Wins\"\nversion: 1\ntags: [\"a\"]\n---\nBody.";
        f.k.create_note("Wins", md, Some(&[])).await.unwrap();
        let note = f.k.entity(&f.note_id("Wins").await).await.unwrap();
        assert!(tag_ids(&note).is_empty(), "{:?}", note.properties);
        assert_eq!(
            f.entity_count().await,
            before + 1,
            "no Tag \"a\" is created"
        );

        let other = ["b".to_string()];
        f.k.create_note("Other", md, Some(&other)).await.unwrap();
        let note = f.k.entity(&f.note_id("Other").await).await.unwrap();
        assert_eq!(tag_ids(&note), [tag_id(&f, "b").await]);

        f.k.create_note("Copied", md, None).await.unwrap();
        let note = f.k.entity(&f.note_id("Copied").await).await.unwrap();
        assert_eq!(
            tag_ids(&note),
            [tag_id(&f, "a").await],
            "without the argument, the frontmatter's tags are used"
        );
    }

    #[tokio::test]
    async fn create_note_rejects_malformed_frontmatter_tags() {
        let f = fixture().await;
        let before = f.entity_count().await;
        for value in ["ml", "[\"ml\", 3]", "[ml]"] {
            let md = format!("---\nname: \"Bad\"\ntags: {value}\n---\nAbout #x.");
            let failure = err(f.k.create_note("Bad", &md, None).await);
            assert!(
                failure.contains("tags:") && failure.contains("JSON list"),
                "{failure}"
            );
        }
        assert_eq!(f.entity_count().await, before, "not even the #x Tag exists");
        assert!(err(f.k.get_note(NoteKey::Name("Bad")).await).starts_with("No note named"));
    }

    fn tag_args(names: &[&str]) -> Vec<String> {
        names.iter().map(std::string::ToString::to_string).collect()
    }

    #[tokio::test]
    async fn tag_entity_adds_and_removes() {
        let f = fixture().await;
        f.create("Tag", "ml").await;
        f.k.create_note("X", "Body.", None).await.unwrap();

        let added = f.k.tag_entity("X", &tag_args(&["ml", "new"]), &[]).await;
        assert_eq!(added.unwrap(), "Tags on \"X\": ml, new.");
        let note = f.k.entity(&f.note_id("X").await).await.unwrap();
        assert_eq!(
            tag_ids(&note),
            [tag_id(&f, "ml").await, tag_id(&f, "new").await],
            "`new` is created"
        );

        let count = f.entity_count().await;
        let removed = f.k.tag_entity("X", &[], &tag_args(&["ml", "absent"])).await;
        assert_eq!(removed.unwrap(), "Tags on \"X\": new.");
        let note = f.k.entity(&f.note_id("X").await).await.unwrap();
        assert_eq!(tag_ids(&note), [tag_id(&f, "new").await]);
        assert_eq!(f.entity_count().await, count, "removing creates no Tag");
        assert!(f
            .k
            .get_note(NoteKey::Name("X"))
            .await
            .unwrap()
            .contains("\ntags: [\"new\"]\n"));

        let cleared = f.k.tag_entity("X", &[], &tag_args(&["NEW"])).await;
        assert_eq!(cleared.unwrap(), "Tags on \"X\": none.");
        let again = f.k.tag_entity("X", &tag_args(&["ML", "ml"]), &[]).await;
        assert_eq!(again.unwrap(), "Tags on \"X\": ml.", "repeats are folded");
    }

    #[tokio::test]
    async fn tag_entity_keeps_a_concurrent_tag() {
        let f = fixture().await;
        f.k.create_note("X", "Body.", None).await.unwrap();
        let id = f.note_id("X").await;

        f.k.tag_entity("X", &tag_args(&["a"]), &[]).await.unwrap();
        f.create("Tag", "b").await;
        let req = AddRelationTargetRequest {
            entity_id: id.clone(),
            property_id: "tags".into(),
            target_id: tag_id(&f, "b").await,
        };
        f.entity
            .add_relation_target(Request::new(req))
            .await
            .unwrap();

        let result = f.k.tag_entity("X", &tag_args(&["c"]), &[]).await;
        assert_eq!(result.unwrap(), "Tags on \"X\": a, b, c.");
        let result = f.k.tag_entity("X", &[], &tag_args(&["a"])).await;
        assert_eq!(result.unwrap(), "Tags on \"X\": b, c.");
    }

    #[tokio::test]
    async fn tag_entity_errors() {
        let f = fixture().await;
        let before = f.entity_count().await;
        let missing = err(f.k.tag_entity("Ghost", &tag_args(&["x"]), &[]).await);
        assert!(missing.contains("No note named"), "{missing}");
        assert_eq!(f.entity_count().await, before, "no note and no Tag");

        f.k.create_note("X", "Body.", None).await.unwrap();
        let before = f.entity_count().await;
        let empty = err(f.k.tag_entity("X", &[], &[]).await);
        assert!(empty.contains("at least one"), "{empty}");
        let both = err(f
            .k
            .tag_entity("X", &tag_args(&["fresh", "ML"]), &tag_args(&["ml"]))
            .await);
        assert!(
            both.contains("\"ML\"") && both.contains("both add and remove"),
            "{both}"
        );
        assert_eq!(f.entity_count().await, before, "nothing was written");
        let note = f.k.entity(&f.note_id("X").await).await.unwrap();
        assert!(tag_ids(&note).is_empty());
    }

    #[tokio::test]
    async fn a_body_hashtag_references_the_tag_without_tagging_the_note() {
        let f = fixture().await;
        f.k.create_note("Mentions", "About #x.", None)
            .await
            .unwrap();
        let note = f.k.entity(&f.note_id("Mentions").await).await.unwrap();
        assert!(tag_ids(&note).is_empty(), "{:?}", note.properties);
        let x = tag_id(&f, "x").await;
        let links: Vec<(&str, &str)> = note
            .links
            .iter()
            .map(|l| {
                (
                    l.source_property_id.as_str(),
                    l.target.as_ref().unwrap().id.as_str(),
                )
            })
            .collect();
        assert_eq!(links, [("content", x.as_str())]);
    }

    #[tokio::test]
    async fn json_rpc_speaks_mcp() {
        let f = fixture().await;
        let rpc = |message: Value| handle_message(&f.k, message);

        let init = rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }))
            .await
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "calcifer");
        let unknown_version = rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }))
            .await
            .unwrap();
        assert_eq!(
            unknown_version["result"]["protocolVersion"],
            PROTOCOL_VERSIONS[0]
        );

        assert!(
            rpc(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .await
                .is_none()
        );

        let listed = rpc(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
            .await
            .unwrap();
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "search_notes",
                "get_note",
                "query_text_location",
                "edit_note",
                "create_note",
                "append_to_note",
                "tag_entity",
                "link_notes",
                "unlink_notes",
                "get_backlinks",
                "create_daily_note",
                "append_to_daily_note",
                "list_structures",
            ]
        );

        let call = |name: &str, arguments: Value| {
            rpc(
                json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
            )
        };
        call("create_note", json!({ "name": "B", "markdown": "" }))
            .await
            .unwrap();
        let created = call(
            "create_note",
            json!({ "name": "A", "markdown": "See [[B]]." }),
        )
        .await
        .unwrap();
        assert_eq!(created["result"]["isError"], false);
        assert!(created["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Saved note \"A\""));
        let tagged = call(
            "create_note",
            json!({ "name": "C", "markdown": "", "tags": ["ml"] }),
        )
        .await
        .unwrap();
        assert!(
            tagged["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .ends_with("tags: 1."),
            "{tagged}"
        );

        let failed = call("append_to_note", json!({ "name": "Nope", "markdown": "x" }))
            .await
            .unwrap();
        assert_eq!(
            failed["result"]["isError"], true,
            "a failing tool is a result, not a protocol error"
        );

        assert_eq!(
            call("no_such_tool", json!({})).await.unwrap()["error"]["code"],
            INVALID_PARAMS
        );
        assert_eq!(
            call("get_note", json!({ "name": 5 })).await.unwrap()["error"]["code"],
            INVALID_PARAMS
        );
        let start_zero = call(
            "edit_note",
            json!({ "name": "A", "start": 0, "end": 0, "markdown": "", "version": 0 }),
        )
        .await
        .unwrap();
        assert_eq!(start_zero["result"]["isError"], true, "{start_zero}");
        let not_an_object = rpc(json!(42)).await.unwrap();
        assert_eq!(
            (&not_an_object["id"], &not_an_object["error"]["code"]),
            (&Value::Null, &json!(INVALID_REQUEST))
        );
        assert_eq!(
            rpc(json!({ "jsonrpc": "2.0", "id": 4, "method": "nope" }))
                .await
                .unwrap()["error"]["code"],
            METHOD_NOT_FOUND
        );
    }

    #[tokio::test]
    async fn json_rpc_tag_entity_tags_a_note() {
        let f = fixture().await;
        let call = |name: &str, arguments: Value| {
            handle_message(
                &f.k,
                json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
            )
        };
        let text = |reply: &Value| {
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_string()
        };

        call("create_note", json!({ "name": "A", "markdown": "Body." }))
            .await
            .unwrap();
        let tagged = call(
            "tag_entity",
            json!({ "name": "A", "add": ["ml", "papers"] }),
        )
        .await
        .unwrap();
        assert_eq!(tagged["result"]["isError"], false, "{tagged}");
        assert_eq!(text(&tagged), "Tags on \"A\": ml, papers.");
        let untagged = call("tag_entity", json!({ "name": "A", "remove": ["ml"] }))
            .await
            .unwrap();
        assert_eq!(text(&untagged), "Tags on \"A\": papers.");

        let none_given = call("tag_entity", json!({ "name": "A" })).await.unwrap();
        assert_eq!(none_given["result"]["isError"], true, "{none_given}");
        let no_name = call("tag_entity", json!({ "add": ["ml"] })).await.unwrap();
        assert_eq!(no_name["error"]["code"], INVALID_PARAMS);
        let read = call("get_note", json!({ "name": "A" })).await.unwrap();
        assert!(text(&read).contains("\ntags: [\"papers\"]\n"), "{read}");
    }

    #[tokio::test]
    async fn json_rpc_unlink_notes_removes_a_relationship() {
        let f = fixture().await;
        let call = |name: &str, arguments: Value| {
            handle_message(
                &f.k,
                json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
            )
        };
        let text = |reply: &Value| {
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_string()
        };

        call("create_note", json!({ "name": "A", "markdown": "Body." }))
            .await
            .unwrap();
        call("create_note", json!({ "name": "B", "markdown": "" }))
            .await
            .unwrap();
        let linked = call("link_notes", json!({ "from": "A", "to": "B" }))
            .await
            .unwrap();
        assert_eq!(linked["result"]["isError"], false, "{linked}");
        assert_eq!(text(&linked), "Related \"A\" -> \"B\".");
        let read = call("get_note", json!({ "name": "A" })).await.unwrap();
        assert!(text(&read).contains("\nrelated: [\"B\"]\n"), "{read}");

        let unlinked = call("unlink_notes", json!({ "from": "A", "to": "B" }))
            .await
            .unwrap();
        assert_eq!(unlinked["result"]["isError"], false, "{unlinked}");
        assert_eq!(text(&unlinked), "Unrelated \"A\" -> \"B\".");
        let again = call("unlink_notes", json!({ "from": "A", "to": "B" }))
            .await
            .unwrap();
        assert_eq!(again["result"]["isError"], false, "{again}");
        assert_eq!(text(&again), "\"A\" wasn't related to \"B\".");
        let read = call("get_note", json!({ "name": "A" })).await.unwrap();
        assert!(text(&read).contains("\nrelated: []\n"), "{read}");

        let missing = call("unlink_notes", json!({ "from": "A", "to": "Ghost" }))
            .await
            .unwrap();
        assert_eq!(missing["result"]["isError"], true, "{missing}");
        assert!(
            text(&missing).starts_with("No note named or with id"),
            "{missing}"
        );
        let no_to = call("unlink_notes", json!({ "from": "A" })).await.unwrap();
        assert_eq!(no_to["error"]["code"], INVALID_PARAMS);
    }

    /// The status `POST /mcp` answers through the local-only layers, sent with
    /// these headers and a ping as the body.
    async fn post_status(f: &Fixture, headers: &[(&str, &str)]) -> StatusCode {
        use axum::body::Body;
        use tower::ServiceExt;

        let app =
            crate::security::local::mcp(router(f.k.clone()), "127.0.0.1:8081".parse().unwrap());
        let mut req = axum::http::Request::post("/mcp");
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let body = Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        app.oneshot(req.body(body).unwrap()).await.unwrap().status()
    }

    #[tokio::test]
    async fn the_router_answers_only_this_machine() {
        let f = fixture().await;
        let (host, json) = (
            ("host", "localhost:8081"),
            ("content-type", "application/json"),
        );

        // Claude Code: no Origin, a JSON body.
        assert_eq!(post_status(&f, &[host, json]).await, StatusCode::OK);
        let inspector = ("origin", "http://localhost:6274");
        assert_eq!(
            post_status(&f, &[host, json, inspector]).await,
            StatusCode::OK
        );

        let rebound = ("host", "evil.example:8081");
        assert_eq!(
            post_status(&f, &[rebound, json]).await,
            StatusCode::FORBIDDEN
        );
        let foreign = ("origin", "https://evil.example");
        assert_eq!(
            post_status(&f, &[host, json, foreign]).await,
            StatusCode::FORBIDDEN
        );
        // A no-cors form post from another site.
        let text = ("content-type", "text/plain");
        assert_eq!(
            post_status(&f, &[host, text]).await,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }
}
