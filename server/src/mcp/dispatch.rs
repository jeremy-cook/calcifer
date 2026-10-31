use serde_json::Value;

use crate::mcp::tools::{Knowledge, NoteKey, ToolError};
use calcifer::v1::SearchMode;

use super::args::{
    CreateNoteArgs, DailyArgs, DateArgs, EditArgs, GetNoteArgs, LinkArgs, Mode, NameArgs, NoArgs,
    NoteArgs, QueryTextArgs, SearchArgs, TagArgs,
};
use super::parse_args;

/// The tool's parsed arguments; arguments that don't fit fail the tool.
macro_rules! args {
    ($tool:expr, $args:expr) => {
        match parse_args($tool, $args) {
            Ok(parsed) => parsed,
            Err(e) => return Ok(Err(e)),
        }
    };
}

/// Runs the named tool. The outer error is a request the server can't read
/// (unknown tool, bad arguments); the inner one is the tool failing.
pub(super) async fn dispatch(
    k: &Knowledge,
    name: &str,
    args: Value,
) -> Result<Result<String, ToolError>, String> {
    Ok(match name {
        "search_notes" => {
            let a: SearchArgs = args!(name, args);
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
            let a: GetNoteArgs = args!(name, args);
            match NoteKey::from_args(a.name.as_deref(), a.date.as_deref()) {
                Ok(key) => k.get_note(key).await,
                Err(e) => Err(e),
            }
        }
        "query_text_location" => {
            let a: QueryTextArgs = args!(name, args);
            match NoteKey::from_args(a.name.as_deref(), a.date.as_deref()) {
                Ok(key) => k.query_text_location(key, &a.query).await,
                Err(e) => Err(e),
            }
        }
        "edit_note" => {
            let a: EditArgs = args!(name, args);
            match NoteKey::from_args(a.name.as_deref(), a.date.as_deref()) {
                Ok(key) => {
                    k.edit_note(key, a.start, a.end, &a.markdown, a.version)
                        .await
                }
                Err(e) => Err(e),
            }
        }
        "create_note" => {
            let a: CreateNoteArgs = args!(name, args);
            k.create_note(&a.name, &a.markdown, a.tags.as_deref()).await
        }
        "append_to_note" => {
            let a: NoteArgs = args!(name, args);
            k.append_to_note(&a.name, &a.markdown).await
        }
        "tag_entity" => {
            let a: TagArgs = args!(name, args);
            k.tag_entity(&a.name, &a.add, &a.remove).await
        }
        "link_notes" => {
            let a: LinkArgs = args!(name, args);
            k.link_notes(&a.from, &a.to).await
        }
        "unlink_notes" => {
            let a: LinkArgs = args!(name, args);
            k.unlink_notes(&a.from, &a.to).await
        }
        "get_backlinks" => {
            let a: NameArgs = args!(name, args);
            k.get_backlinks(&a.name).await
        }
        "create_daily_note" => {
            let a: DateArgs = args!(name, args);
            k.create_daily_note(&a.date).await
        }
        "append_to_daily_note" => {
            let a: DailyArgs = args!(name, args);
            k.append_to_daily_note(&a.date, &a.markdown).await
        }
        "list_structures" => {
            let NoArgs {} = args!(name, args);
            Ok(k.list_structures())
        }
        _ => return Err(format!("unknown tool {name}")),
    })
}
