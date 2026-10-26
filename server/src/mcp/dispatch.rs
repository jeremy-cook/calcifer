use serde_json::Value;

use crate::mcp::tools::{Knowledge, NoteKey, ToolError};
use calcifer::v1::SearchMode;

use super::args::{
    CreateNoteArgs, DailyArgs, DateArgs, EditArgs, GetNoteArgs, LinkArgs, Mode, NameArgs, NoteArgs,
    QueryTextArgs, SearchArgs, TagArgs,
};
use super::parse_args;

/// Runs the named tool. The outer error is a request the server can't read
/// (unknown tool, bad arguments); the inner one is the tool failing.
pub(super) async fn dispatch(
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
