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
use tools::ToolError;

/// How note markdown refers to other entities; shared by the tools that read or write it.
const LINK_SYNTAX: &str = "[[Name]] links an existing note; [[Structure/Name]] links an existing entity of another structure from list_structures, e.g. [[Todo/Ship]] or [[DailyNote/June 13, 2026]]; #tag applies a tag (created if missing). A link to anything that doesn't exist fails the call and writes nothing, so create the target first. These become real graph links automatically.";

/// Newest first. A client's version is echoed back when we speak it.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const PARSE_ERROR: i64 = -32700;

/// `POST /mcp`. Any other method on the path is answered 405 by the router.
pub fn router(knowledge: Knowledge) -> axum::Router {
    axum::Router::new().route("/mcp", post(post_mcp)).with_state(knowledge)
}

async fn post_mcp(State(knowledge): State<Knowledge>, body: Bytes) -> Response {
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return json_response(error_reply(Value::Null, PARSE_ERROR, "invalid JSON"));
    };
    // A batch is answered with a batch of the replies; notifications have none.
    let reply = match message {
        Value::Array(messages) if messages.is_empty() => Some(error_reply(Value::Null, INVALID_REQUEST, "empty batch")),
        Value::Array(messages) => {
            let replies: Vec<Value> = futures::future::join_all(messages.into_iter().map(|m| handle_message(&knowledge, m)))
                .await
                .into_iter()
                .flatten()
                .collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        message => handle_message(&knowledge, message).await,
    };
    match reply {
        Some(reply) => json_response(reply),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn json_response(body: Value) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body.to_string()).into_response()
}

fn error_reply(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// One JSON-RPC message in, its reply out. Notifications and stray responses
/// (no `method`, or no `id`) get none; anything that isn't an object is an error.
async fn handle_message(knowledge: &Knowledge, message: Value) -> Option<Value> {
    if !message.is_object() {
        return Some(error_reply(Value::Null, INVALID_REQUEST, "not a JSON-RPC message"));
    }
    let id = message.get("id").cloned();
    let Some(method) = message["method"].as_str() else {
        return id.map(|id| error_reply(id, INVALID_REQUEST, "not a JSON-RPC request"));
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
        Err((code, message)) => error_reply(id, code, &message),
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
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
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
struct QueryTextArgs {
    name: String,
    query: String,
}

#[derive(Deserialize)]
struct EditArgs {
    name: String,
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
async fn dispatch(k: &Knowledge, name: &str, args: Value) -> Result<Result<String, ToolError>, String> {
    Ok(match name {
        "search_notes" => {
            let a: SearchArgs = parse_args(name, args)?;
            let mode = match a.mode.unwrap_or(Mode::Hybrid) {
                Mode::Lexical => SearchMode::Lexical,
                Mode::Semantic => SearchMode::Semantic,
                Mode::Hybrid => SearchMode::Hybrid,
            };
            match a.limit {
                Some(0) => return Err("invalid arguments for search_notes: limit must be positive".into()),
                limit => k.search_notes(&a.query, limit.unwrap_or(10), mode).await,
            }
        }
        "get_note" => k.get_note(&parse_args::<NameArgs>(name, args)?.name).await,
        "query_text_location" => {
            let a: QueryTextArgs = parse_args(name, args)?;
            k.query_text_location(&a.name, &a.query).await
        }
        "edit_note" => {
            let a: EditArgs = parse_args(name, args)?;
            k.edit_note(&a.name, a.start, a.end, &a.markdown, a.version).await
        }
        "create_note" => {
            let a: NoteArgs = parse_args(name, args)?;
            k.create_note(&a.name, &a.markdown).await
        }
        "append_to_note" => {
            let a: NoteArgs = parse_args(name, args)?;
            k.append_to_note(&a.name, &a.markdown).await
        }
        "link_notes" => {
            let a: LinkArgs = parse_args(name, args)?;
            k.link_notes(&a.from, &a.to).await
        }
        "get_backlinks" => k.get_backlinks(&parse_args::<NameArgs>(name, args)?.name).await,
        "create_daily_note" => k.create_daily_note(&parse_args::<DateArgs>(name, args)?.date).await,
        "append_to_daily_note" => {
            let a: DailyArgs = parse_args(name, args)?;
            k.append_to_daily_note(&a.date, &a.markdown).await
        }
        "list_structures" => k.list_structures(),
        _ => return Err(format!("unknown tool {name}")),
    })
}

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
            format!("Read a note by name. The result is YAML frontmatter (name, version, linked_from: the notes that link to it) followed by the note's markdown body. Links are written with each target's current name: {LINK_SYNTAX} create_note ignores the frontmatter, so the result can be copied into a new note."),
            json!({ "name": string }),
            &["name"],
        ),
        tool(
            "query_text_location",
            "Find where text occurs in an existing note. Returns the note's version and, for each block containing a match, its line range and text. The ranges are whole blocks, ready to pass to edit_note as start and end, so you never have to count lines. The match is exact and case-sensitive against the markdown get_note shows (links as [[Name]]); quote enough text to be unique.".into(),
            json!({ "name": string, "query": string }),
            &["name", "query"],
        ),
        tool(
            "edit_note",
            format!("Replace a range of lines in an existing note's body. Get the lines and version from query_text_location, or read the note with get_note: lines are counted in the body only, from 1 at the first line after the closing --- of the frontmatter, and version is the frontmatter's version. A paragraph, a heading, a whole list and a whole code block are each one block, and blocks are separated by a blank line; start and end must not fall inside a block. Lines start..end (inclusive) are replaced by the markdown; empty markdown deletes them; end = start - 1 inserts before line start (start = last line + 1 appends). A <!-- calcifer:<type> --> line stands for content markdown can't express (a table, say), and formatting markdown can't express (underline, colour, alignment, ...) is shown as plain text: a range that covers either is refused, so edit around it. Fails if the note changed since you read it: read again and redo the edit. In the markdown, {LINK_SYNTAX}"),
            json!({
                "name": string,
                "start": { "type": "integer", "exclusiveMinimum": 0 },
                "end": { "type": "integer", "minimum": 0 },
                "markdown": string,
                "version": { "type": "integer", "minimum": 0 },
            }),
            &["name", "start", "end", "markdown", "version"],
        ),
        tool(
            "create_note",
            format!("Create a new note. Fails if a note with that name already exists; change an existing note with edit_note or append_to_note. In the markdown body, {LINK_SYNTAX} Search first to avoid duplicates. Copying get_note output into a new note drops its <!-- calcifer:<type> --> placeholder blocks and any formatting markdown can't express."),
            json!({ "name": string, "markdown": string }),
            &["name", "markdown"],
        ),
        tool(
            "append_to_note",
            format!("Append markdown to an existing note, growing it across turns. Fails if there is no note with that name; use create_note to make one. In the appended text, {LINK_SYNTAX}"),
            json!({ "name": string, "markdown": string }),
            &["name", "markdown"],
        ),
        tool(
            "link_notes",
            "Link one note to another by adding a [[wikilink]] reference to the first note.".into(),
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
            "List the entity types (structures) available in this knowledge base.".into(),
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
        resolve_entity_request, CreateEntityRequest, ListEntitiesRequest, RenameEntityRequest, ResolveEntityRequest,
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
        Fixture { k: Knowledge::new(entity.clone(), richtext, search), entity }
    }

    impl Fixture {
        async fn note_id(&self, name: &str) -> String {
            let req = ResolveEntityRequest {
                structure_type: "Note".into(),
                key: Some(resolve_entity_request::Key::Name(name.into())),
                create_if_missing: false,
            };
            self.entity.resolve_entity(Request::new(req)).await.unwrap().into_inner().entity.unwrap().id
        }

        async fn entity_count(&self) -> usize {
            let req = ListEntitiesRequest { structure_type: String::new() };
            self.entity.list_entities(Request::new(req)).await.unwrap().into_inner().entities.len()
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
                .map(|l| format!("{}:{}", l.target.as_ref().unwrap().structure_type, l.target.as_ref().unwrap().id))
                .collect();
            targets.sort();
            targets
        }

        async fn create(&self, structure_type: &str, name: &str) {
            let req = CreateEntityRequest {
                structure_type: structure_type.into(),
                name: Some(name.into()),
                properties: Default::default(),
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
        note.lines().find_map(|l| l.strip_prefix("version: ")).unwrap().parse().unwrap()
    }

    fn err(result: Result<String, ToolError>) -> String {
        result.expect_err("expected the tool to fail").0
    }

    #[tokio::test]
    async fn a_note_with_a_link_and_a_tag_derives_two_links_and_backlinks() {
        let f = fixture().await;
        f.k.create_note("Attention", "").await.unwrap();
        f.k.create_note("Positional Encoding", "").await.unwrap();
        let saved = f.k.create_note("Transformers", "An architecture built on [[Attention]]. #ml").await.unwrap();
        assert!(saved.contains("Wikilinks: 2 [") && saved.contains("Note") && saved.contains("Tag"), "{saved}");

        assert_eq!(f.k.get_backlinks("Attention").await.unwrap(), "- Transformers");
        assert_eq!(f.k.get_backlinks("Transformers").await.unwrap(), "Nothing links to \"Transformers\" yet.");
        assert_eq!(f.k.get_backlinks("Nowhere").await.unwrap(), "No note named \"Nowhere\".");

        let note = f.k.get_note("Transformers").await.unwrap();
        assert!(note.contains("[[Attention]]") && note.contains("#ml"), "{note}");

        f.k.append_to_note("Transformers", "They use [[Positional Encoding]] too.").await.unwrap();
        let note = f.k.get_note("Transformers").await.unwrap();
        assert!(note.contains("[[Positional Encoding]]") && note.contains("[[Attention]]"), "{note}");
        assert_eq!(f.k.get_backlinks("Positional Encoding").await.unwrap(), "- Transformers");
    }

    #[tokio::test]
    async fn search_finds_notes_by_prefix() {
        let f = fixture().await;
        f.k.create_note("Transformers", "An architecture.").await.unwrap();
        let found = f.k.search_notes("trans", 10, SearchMode::Hybrid).await.unwrap();
        assert!(found.contains("[Note] Transformers"), "{found}");
        assert_eq!(f.k.search_notes("zzzz", 10, SearchMode::Lexical).await.unwrap(), "No matches for \"zzzz\".");
    }

    #[tokio::test]
    async fn writing_to_a_missing_note_is_an_error_and_creates_nothing() {
        let f = fixture().await;
        let before = f.entity_count().await;
        for failure in [
            err(f.k.append_to_note("Ghost", "x").await),
            err(f.k.link_notes("Ghost", "Attention").await),
            err(f.k.query_text_location("Ghost", "x").await),
            err(f.k.edit_note("Ghost", 1, 1, "x", 0).await),
        ] {
            assert!(failure.contains("No note named"), "{failure}");
        }
        assert_eq!(f.entity_count().await, before);
    }

    #[tokio::test]
    async fn daily_notes_are_one_per_day() {
        let f = fixture().await;
        f.k.create_note("Transformers", "x").await.unwrap();
        let created = f.k.create_daily_note("2026-06-13").await.unwrap();
        assert!(created.contains("\"June 13, 2026\"") && created.ends_with("for 2026-06-13 created."), "{created}");
        let again = f.k.create_daily_note("2026-06-13").await.unwrap();
        assert!(again.ends_with("for 2026-06-13 already existed."), "{again}");

        f.k.append_to_daily_note("2026-06-13", "Read about [[Transformers]] today.").await.unwrap();
        assert!(f.k.get_backlinks("Transformers").await.unwrap().contains("June 13, 2026"));
    }

    #[tokio::test]
    async fn get_note_writes_chips_from_the_targets_current_names() {
        let f = fixture().await;
        f.k.create_note("Old Name", "").await.unwrap();
        f.k.create_note("Source", "Points at [[Old Name]].").await.unwrap();
        let id = f.note_id("Old Name").await;
        let rename = RenameEntityRequest { id, name: "New Name".into() };
        f.entity.rename_entity(Request::new(rename)).await.unwrap();
        let note = f.k.get_note("Source").await.unwrap();
        assert!(note.contains("[[New Name]]") && !note.contains("Old Name"), "{note}");
    }

    #[tokio::test]
    async fn get_note_copied_into_create_note_links_the_same_targets() {
        let f = fixture().await;
        f.create("Todo", "Ship").await;
        f.k.create_daily_note("2026-06-13").await.unwrap();
        f.k.create_note("RT Note", "").await.unwrap();
        f.k.create_note("RT Alias", "").await.unwrap();
        let md = "Uses [[RT Note]] and [[RT Alias|the alias]]. #rt\n\n- do [[Todo/Ship]] on [[DailyNote/June 13, 2026]]";
        f.k.create_note("Round Trip", md).await.unwrap();
        let id = f.note_id("Round Trip").await;
        let links_before = f.link_targets(&id).await;
        assert_eq!(links_before.len(), 5, "{links_before:?}");
        assert!(links_before.iter().any(|l| l.starts_with("Todo:")) && links_before.iter().any(|l| l.starts_with("DailyNote:")));

        let count = f.entity_count().await;
        let note = f.k.get_note("Round Trip").await.unwrap();
        assert!(note.contains("[[Todo/Ship]]") && note.contains("[[DailyNote/June 13, 2026]]"), "{note}");
        assert!(note.contains("[[RT Alias]]") && !note.contains("the alias") && note.contains("#rt"), "{note}");
        assert!(note.starts_with("---\nname: \"Round Trip\"\nversion: "), "{note}");
        assert!(note.contains("\nlinked_from: []\n---\n"), "{note}");

        // The whole result, frontmatter included, goes into a new note.
        f.k.create_note("Round Trip Copy", &note).await.unwrap();
        assert_eq!(body(&f.k.get_note("Round Trip Copy").await.unwrap()), body(&note));
        assert_eq!(f.link_targets(&f.note_id("Round Trip Copy").await).await, links_before);
        assert_eq!(f.entity_count().await, count + 1, "only the copy itself is created");
    }

    #[tokio::test]
    async fn create_note_refuses_an_existing_note() {
        let f = fixture().await;
        f.k.create_note("Existing", "Original.").await.unwrap();
        let refused = err(f.k.create_note("Existing", "Replacement.").await);
        assert!(refused.contains("already exists"), "{refused}");
        assert!(err(f.k.create_note("existing", "Case.").await).contains("already exists"), "names match case-insensitively");
        assert_eq!(body(&f.k.get_note("Existing").await.unwrap()), "Original.");

        f.k.create_note("Blank", "").await.unwrap();
        assert!(err(f.k.create_note("Blank", "Filled.").await).contains("already exists"), "even a blank note exists");
    }

    #[tokio::test]
    async fn a_link_to_a_missing_target_fails_the_write_and_creates_nothing() {
        let f = fixture().await;
        f.k.create_note("Existing", "Body.").await.unwrap();
        let before = f.entity_count().await;
        let md = "See [[Attenton]], [[Todo/Ship]] and [[DailyNote/February 1, 1901]].";
        for failure in [
            err(f.k.create_note("New", md).await),
            err(f.k.append_to_note("Existing", md).await),
            err(f.k.append_to_daily_note("2026-06-13", md).await),
            err(f.k.edit_note("Existing", 1, 1, md, version(&f.k.get_note("Existing").await.unwrap())).await),
        ] {
            assert!(failure.contains("Nothing was written: 3 link(s)"), "{failure}");
            assert!(failure.contains("[[Attenton]], [[Todo/Ship]], [[DailyNote/February 1, 1901]]"), "{failure}");
        }
        assert!(err(f.k.link_notes("Existing", "Nowhere").await).contains("[[Nowhere]]"));
        assert_eq!(f.entity_count().await, before, "not even append_to_daily_note's day");
        assert_eq!(body(&f.k.get_note("Existing").await.unwrap()), "Body.");
        assert!(f.k.get_note("New").await.unwrap().starts_with("No note named"));
    }

    #[tokio::test]
    async fn a_missing_tag_is_created() {
        let f = fixture().await;
        let saved = f.k.create_note("Tagged", "About #brandnew things.").await.unwrap();
        assert!(saved.contains("Wikilinks: 1 [Tag]"), "{saved}");
    }

    #[tokio::test]
    async fn create_note_may_link_to_itself() {
        let f = fixture().await;
        f.k.create_note("Selfie", "I am [[Selfie]].").await.unwrap();
        let id = f.note_id("Selfie").await;
        assert_eq!(body(&f.k.get_note("Selfie").await.unwrap()), "I am [[Selfie]].");
        assert_eq!(f.link_targets(&id).await, [format!("Note:{id}")]);
    }

    #[tokio::test]
    async fn a_leading_rule_that_is_not_frontmatter_is_note_content() {
        let f = fixture().await;
        f.k.create_note("Rule", "---\nplain text\n---").await.unwrap();
        assert!(body(&f.k.get_note("Rule").await.unwrap()).contains("plain text"));
    }

    #[tokio::test]
    async fn query_text_location_reports_whole_blocks_and_the_version() {
        let f = fixture().await;
        f.k.create_note("Link", "").await.unwrap();
        f.k.create_note("Subject", "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]].").await.unwrap();
        let note = f.k.get_note("Subject").await.unwrap();
        assert_eq!(body(&note), "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]].");
        let v = version(&note);

        let found = f.k.query_text_location("Subject", "two").await.unwrap();
        assert_eq!(found, format!("version: {v}\n\nlines 5-6 (1 match):\n- one\n- two"));
        let found = f.k.query_text_location("Subject", "Alpha.\n\n## Heading").await.unwrap();
        assert!(found.contains("lines 1-1 (1 match):\nAlpha.\n\nlines 3-3 (1 match):\n## Heading"), "{found}");
        let found = f.k.query_text_location("Subject", "o").await.unwrap();
        assert!(found.ends_with("lines 5-6 (2 matches):\n- one\n- two"), "{found}");
        let found = f.k.query_text_location("Subject", "n").await.unwrap();
        let (a, b, c) = (found.find("lines 3-3").unwrap(), found.find("lines 5-6").unwrap(), found.find("lines 8-8").unwrap());
        assert!(a < b && b < c, "{found}");
        assert!(f.k.query_text_location("Subject", "nowhere in the note").await.unwrap().starts_with("No match"));
        assert!(err(f.k.query_text_location("Subject", "").await).contains("query is empty"));
    }

    #[tokio::test]
    async fn edit_note_replaces_inserts_and_deletes_whole_blocks() {
        let f = fixture().await;
        f.k.create_note("Link", "").await.unwrap();
        f.k.create_note("Subject", "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]].").await.unwrap();
        let id = f.note_id("Subject").await;
        let read = || async {
            let note = f.k.get_note("Subject").await.unwrap();
            (body(&note).to_string(), version(&note))
        };

        let (_, v) = read().await;
        f.k.edit_note("Subject", 3, 3, "## Renamed heading", v).await.unwrap();
        let (text, v) = read().await;
        assert_eq!(text, "Alpha.\n\n## Renamed heading\n\n- one\n- two\n\nLast [[Link]].");

        let stale = err(f.k.edit_note("Subject", 1, 1, "Stale", v - 1).await);
        assert!(stale.contains("changed since you read"), "{stale}");
        assert!(!read().await.0.contains("Stale"));

        let inside_start = err(f.k.edit_note("Subject", 6, 6, "x", v).await);
        assert!(inside_start.contains("line 6 is inside the block at lines 5-6") && inside_start.contains("start at 5"), "{inside_start}");
        let inside_end = err(f.k.edit_note("Subject", 5, 5, "x", v).await);
        assert!(inside_end.contains("end at 6"), "{inside_end}");
        assert!(err(f.k.edit_note("Subject", 2, 2, "x", v).await).contains("contain no block"));
        assert_eq!(read().await.0, text, "a rejected range writes nothing");

        f.k.edit_note("Subject", 1, 0, "First.", v).await.unwrap();
        let (text, v) = read().await;
        assert!(text.starts_with("First.\n\nAlpha.\n\n## Renamed heading"), "{text}");

        // Blank lines at either edge of a range are fine.
        f.k.edit_note("Subject", 2, 4, "Alpha two.", v).await.unwrap();
        let (text, v) = read().await;
        assert!(text.starts_with("First.\n\nAlpha two.\n\n## Renamed heading"), "{text}");

        // Now: First. 1, Alpha two. 3, heading 5, list 7-8, Last 10.
        assert_eq!(f.link_targets(&id).await.len(), 1);
        f.k.edit_note("Subject", 10, 10, "Last, rewritten.", v).await.unwrap();
        assert!(f.link_targets(&id).await.is_empty(), "replacing the block that held a link drops the link");

        let (_, v) = read().await;
        f.k.edit_note("Subject", 5, 8, "", v).await.unwrap();
        let (text, v) = read().await;
        assert_eq!(text, "First.\n\nAlpha two.\n\nLast, rewritten.");

        f.k.edit_note("Subject", 6, 5, "Appended.", v).await.unwrap();
        let (text, v) = read().await;
        assert_eq!(text, "First.\n\nAlpha two.\n\nLast, rewritten.\n\nAppended.");

        assert!(err(f.k.edit_note("Subject", 99, 99, "x", v).await).contains("start 99 is outside"));
        assert!(err(f.k.edit_note("Subject", 1, 99, "x", v).await).contains("end 99 is outside"));
        assert!(err(f.k.edit_note("Subject", 0, 0, "x", v).await).contains("start must be at least 1"));
        assert!(err(f.k.edit_note("Subject", 1, 1, "x", -1).await).contains("version at least 0"));
        assert!(err(f.k.edit_note("Subject", 3, 1, "x", v).await).contains("end 1 is outside"));

        f.k.edit_note("Subject", 1, 7, "", v).await.unwrap();
        let (text, v) = read().await;
        assert_eq!(text, "", "deleting every block leaves an empty body");
        f.k.edit_note("Subject", 1, 0, "Fresh.", v).await.unwrap();
        assert_eq!(read().await.0, "Fresh.");
        let (doc, _) = f.k.get_doc(&tools::doc_ref("Note", &id).unwrap()).await.unwrap();
        assert_eq!(doc["content"].as_array().unwrap().len(), 1, "no stray empty paragraph");
    }

    #[tokio::test]
    async fn append_keeps_nodes_the_converter_does_not_know() {
        let f = fixture().await;
        f.k.create_note("Rich", "First.").await.unwrap();
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
        f.k.create_note(name, "").await.unwrap();
        let r = tools::doc_ref("Note", &f.note_id(name).await).unwrap();
        let (_, updated_at) = f.k.get_doc(&r).await.unwrap();
        f.k.put_doc(&r, &json!({ "type": "doc", "content": content }), Some(updated_at)).await.unwrap();
        r
    }

    #[tokio::test]
    async fn edit_note_keeps_an_image_it_inserts_beside() {
        let f = fixture().await;
        let image = json!({ "type": "image", "attrs": { "src": "data:image/png;base64,AAAA", "width": "40%" } });
        let r = note_with_blocks(&f, "Picture", json!([image])).await;
        let note = f.k.get_note("Picture").await.unwrap();
        assert_eq!(body(&note), "<!-- calcifer:image -->");

        f.k.edit_note("Picture", 1, 0, "Caption first.", version(&note)).await.unwrap();
        let (doc, _) = f.k.get_doc(&r).await.unwrap();
        assert_eq!(doc["content"][0]["content"][0]["text"], "Caption first.");
        assert_eq!(doc["content"][1], image, "the image survives the insert");
    }

    #[tokio::test]
    async fn edit_note_refuses_a_range_over_what_markdown_cannot_express() {
        let f = fixture().await;
        let table = json!({ "type": "table", "content": [] });
        let para = |text: &str| json!({ "type": "paragraph", "content": [{ "type": "text", "text": text }] });
        let r = note_with_blocks(&f, "Tabular", json!([para("Intro."), table, para("Outro.")])).await;
        let note = f.k.get_note("Tabular").await.unwrap();
        assert_eq!(body(&note), "Intro.\n\n<!-- calcifer:table -->\n\nOutro.");
        let (before, _) = f.k.get_doc(&r).await.unwrap();

        let refused = err(f.k.edit_note("Tabular", 1, 5, "Everything, rewritten.", version(&note)).await);
        assert!(refused.contains("lines 3-3 hold a table that markdown can't express; leave them out of the range"), "{refused}");
        assert_eq!(f.k.get_doc(&r).await.unwrap().0, before, "a refused edit writes nothing");

        // Ranges beside it are fine.
        f.k.edit_note("Tabular", 5, 5, "Outro, rewritten.", version(&note)).await.unwrap();
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
        let note = f.k.get_note("Styled").await.unwrap();
        assert_eq!(body(&note), "An underlined word.\n\n## Centred\n\nSee [the site](https://example.com).");
        let (before, _) = f.k.get_doc(&r).await.unwrap();

        let underline = err(f.k.edit_note("Styled", 1, 1, "Rewritten.", version(&note)).await);
        assert!(underline.contains("lines 1-1 hold underline formatting that markdown can't express"), "{underline}");
        let centred = err(f.k.edit_note("Styled", 3, 3, "## Rewritten", version(&note)).await);
        assert!(centred.contains("lines 3-3 hold a heading's textAlign that markdown can't express"), "{centred}");
        assert_eq!(f.k.get_doc(&r).await.unwrap().0, before, "a refused edit writes nothing");

        f.k.edit_note("Styled", 5, 5, "Now see [the docs](https://docs.rs/x_(y)).", version(&note)).await.unwrap();
        let (after, _) = f.k.get_doc(&r).await.unwrap();
        let linked = &after["content"][2]["content"][1];
        assert_eq!(linked["text"], "the docs");
        assert_eq!(linked["marks"][0]["type"], "link");
        assert_eq!(linked["marks"][0]["attrs"]["href"], "https://docs.rs/x_(y)");
        assert_eq!(after["content"][0], before["content"][0], "the blocks before it are untouched");
        assert_eq!(after["content"][1], before["content"][1]);
    }

    #[tokio::test]
    async fn list_structures_describes_the_registry() {
        let f = fixture().await;
        let listed = f.k.list_structures().unwrap();
        assert!(listed.contains("- Note: ") && listed.contains("- Todo: "), "{listed}");
        assert!(listed.contains("priority (select: "), "{listed}");
        assert!(listed.contains("(relations -> Tag)"), "{listed}");
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
        assert_eq!(unknown_version["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);

        assert!(rpc(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await.is_none());

        let listed = rpc(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await.unwrap();
        let names: Vec<&str> = listed["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "search_notes", "get_note", "query_text_location", "edit_note", "create_note", "append_to_note",
                "link_notes", "get_backlinks", "create_daily_note", "append_to_daily_note", "list_structures",
            ]
        );

        let call = |name: &str, arguments: Value| {
            rpc(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } }))
        };
        call("create_note", json!({ "name": "B", "markdown": "" })).await.unwrap();
        let created = call("create_note", json!({ "name": "A", "markdown": "See [[B]]." })).await.unwrap();
        assert_eq!(created["result"]["isError"], false);
        assert!(created["result"]["content"][0]["text"].as_str().unwrap().starts_with("Saved note \"A\""));

        let failed = call("append_to_note", json!({ "name": "Nope", "markdown": "x" })).await.unwrap();
        assert_eq!(failed["result"]["isError"], true, "a failing tool is a result, not a protocol error");

        assert_eq!(call("no_such_tool", json!({})).await.unwrap()["error"]["code"], INVALID_PARAMS);
        assert_eq!(call("get_note", json!({})).await.unwrap()["error"]["code"], INVALID_PARAMS);
        let start_zero = call("edit_note", json!({ "name": "A", "start": 0, "end": 0, "markdown": "", "version": 0 })).await.unwrap();
        assert_eq!(start_zero["result"]["isError"], true, "{start_zero}");
        let not_an_object = rpc(json!(42)).await.unwrap();
        assert_eq!((&not_an_object["id"], &not_an_object["error"]["code"]), (&Value::Null, &json!(INVALID_REQUEST)));
        assert_eq!(rpc(json!({ "jsonrpc": "2.0", "id": 4, "method": "nope" })).await.unwrap()["error"]["code"], METHOD_NOT_FOUND);
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

    #[tokio::test]
    async fn the_router_answers_only_this_machine() {
        let f = fixture().await;
        let (host, json) = (("host", "localhost:8081"), ("content-type", "application/json"));

        // Claude Code: no Origin, a JSON body.
        assert_eq!(post_status(&f, &[host, json]).await, StatusCode::OK);
        let inspector = ("origin", "http://localhost:6274");
        assert_eq!(post_status(&f, &[host, json, inspector]).await, StatusCode::OK);

        let rebound = ("host", "evil.example:8081");
        assert_eq!(post_status(&f, &[rebound, json]).await, StatusCode::FORBIDDEN);
        let foreign = ("origin", "https://evil.example");
        assert_eq!(post_status(&f, &[host, json, foreign]).await, StatusCode::FORBIDDEN);
        // A no-cors form post from another site.
        let text = ("content-type", "text/plain");
        assert_eq!(post_status(&f, &[host, text]).await, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
