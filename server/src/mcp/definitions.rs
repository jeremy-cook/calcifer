use serde_json::{json, Value};

/// How note markdown refers to other entities; shared by the tools that read or write it.
/// Wherever a tool takes a note's name, it takes the note's id too.
pub(super) const NOTE_NAME: &str = "A note is given by its name or its id (search_notes and get_note show ids); a name several notes share fails the call, listing their ids to call again with, and changes nothing.";

pub(super) const LINK_SYNTAX: &str = "[[Name]] links an existing note, and [[<id>]] links one by its id; [[Structure/Name]] links an existing entity of another structure from list_structures, e.g. [[DailyNote/June 13, 2026]]; #tag references a tag (created if missing); it doesn't tag the note. Pass `tags` to create_note, or use tag_entity, to tag it. To relate two notes without writing a link into either, use link_notes. A link to anything that doesn't exist fails the call and writes nothing, so create the target first; so does a [[Name]] that several notes share, which must be linked by id, and get_note writes such a link as [[<id>]]. These become real graph links automatically.";

#[allow(
    clippy::needless_pass_by_value,
    reason = "every call site builds the description and schema inline"
)]
pub(super) fn tool(name: &str, description: String, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

pub(super) fn tool_definitions() -> Vec<Value> {
    let string = json!({ "type": "string" });
    vec![
        tool(
            "search_notes",
            "Search existing notes and tags. Each result is its structure, name and id. Use this before creating, to find and reuse existing notes instead of making duplicates. `mode` picks the ranking: \"lexical\" (keyword/FTS5), \"semantic\" (meaning via embeddings), or \"hybrid\" (both, fused — the default and best for natural-language queries).".into(),
            json!({
                "query": string,
                "limit": { "type": "integer", "exclusiveMinimum": 0 },
                "mode": { "type": "string", "enum": ["lexical", "semantic", "hybrid"] },
            }),
            &["query"],
        ),
        tool(
            "get_note",
            format!("Read a note by name (or id), or a daily note by date (yyyy-MM-dd); give one of the two. {NOTE_NAME} The result is YAML frontmatter followed by the markdown body: name, id, version, tags and related for a note (`related` lists the notes it's directly related to, each by name, or by id where several notes share the name; change it with link_notes and unlink_notes); name, id, version and date for a daily note, which has no tags or related. A day with no daily note is an error: use append_to_daily_note to start one. Use get_backlinks for the notes that link to it. Links are written with each target's current name: {LINK_SYNTAX} create_note reads the frontmatter's tags when given no `tags` argument, so the result can be copied into a new note; it ignores `related`, so the copy isn't related to anything."),
            json!({ "name": string, "date": string }),
            &[],
        ),
        tool(
            "query_text_location",
            format!("Find where text occurs in an existing note (by name or id) or daily note (by date, yyyy-MM-dd); give one of the two. {NOTE_NAME} Returns the note's version and, for each block containing a match, its line range and text. The ranges are whole blocks, ready to pass to edit_note as start and end, so you never have to count lines. The match is exact and case-sensitive against the markdown get_note shows (links as [[Name]]); quote enough text to be unique."),
            json!({ "name": string, "date": string, "query": string }),
            &["query"],
        ),
        tool(
            "edit_note",
            format!("Replace a range of lines in the body of an existing note (by name or id) or daily note (by date, yyyy-MM-dd); give one of the two. {NOTE_NAME} Get the lines and version from query_text_location, or read the note with get_note: lines are counted in the body only, from 1 at the first line after the closing --- of the frontmatter, and version is the frontmatter's version. A paragraph, a heading, a whole list and a whole code block are each one block, and blocks are separated by a blank line; start and end must not fall inside a block. Lines start..end (inclusive) are replaced by the markdown; empty markdown deletes them; end = start - 1 inserts before line start (start = last line + 1 appends). A <!-- calcifer:<type> --> line stands for content markdown can't express (a table, say), and formatting markdown can't express (underline, colour, alignment, ...) is shown as plain text: a range that covers either is refused, so edit around it. Fails if the note changed since you read it: read again and redo the edit. In the markdown, {LINK_SYNTAX}"),
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
            format!("Create a new note. Fails if a note with that name already exists, listing its id; change an existing note with edit_note or append_to_note. In the markdown body, {LINK_SYNTAX} `tags` lists tag names (or Tag ids) to tag the note with; each Tag is created if missing. Without a `tags` argument, a `tags:` line in leading frontmatter (as get_note writes it) is used. Search first to avoid duplicates. Copying get_note output into a new note drops its <!-- calcifer:<type> --> placeholder blocks and any formatting markdown can't express."),
            json!({ "name": string, "markdown": string, "tags": { "type": "array", "items": string } }),
            &["name", "markdown"],
        ),
        tool(
            "append_to_note",
            format!("Append markdown to an existing note, growing it across turns. {NOTE_NAME} Fails if there is no such note; use create_note to make one. In the appended text, {LINK_SYNTAX}"),
            json!({ "name": string, "markdown": string }),
            &["name", "markdown"],
        ),
        tool(
            "tag_entity",
            format!("Add or remove tags on an existing note (only notes have tags). {NOTE_NAME} `add` lists tag names (or Tag ids) to tag it with, each Tag created if missing; `remove` lists tag names or ids to untag, and a tag the note doesn't have is ignored. Give at least one. Returns the note's tags afterwards. A #tag in a note's body only references the Tag; this is how a note is tagged."),
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
            format!("List the notes that link to a given note, each with its id. {NOTE_NAME}"),
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
