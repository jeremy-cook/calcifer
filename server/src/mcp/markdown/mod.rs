//! Markdown <-> TipTap JSON, for the agent's tools. Agents write markdown with
//! [[wikilinks]], #tags and ISO dates; `parse` emits editor-faithful
//! mention/hashtag/dateChip nodes whose ids the caller resolves by name, so
//! `RichTextService.PutRichText` derives the graph. `block_markdown` goes the
//! other way, writing each chip from its target's current name.
//!
//! [[Name]] and [[Name|label]] mention a Note; [[Structure/Name]] mentions
//! another structure when Structure is exactly a registry type (e.g.
//! [[Tag/ship]]). Emphasis (`**bold**`, `*italic*`), `~~strike~~`, inline
//! `code` and `[text](href)` links become ProseMirror marks. `_` is deliberately
//! not emphasis, so snake_case survives. `\X` is a literal X (for punctuation or
//! a digit), and text that would read as syntax is written escaped, so markdown
//! -> doc -> markdown gives back what went in.
//!
//! Lists nest two spaces a level; `- [ ] ` and `- [x] ` are task items. A line
//! ending in `\` is a hard break, and a bare `>` separates a quote's paragraphs.
//!
//! A horizontal rule is `---` and an image `![alt](src "title")`, each on its
//! own line. A node with no markdown form is written as one placeholder line,
//! `<!-- calcifer:<type> -->`, so the agent can see it; reading markdown skips
//! placeholders. Other marks (underline, colour, ...) are written as their plain
//! text. `inexpressible` tells edits which blocks hold either, to leave alone.
//!
//! Docs are `serde_json::Value`s: a stored doc can carry nodes and marks this
//! converter doesn't know, and they must pass through an append untouched.
//!
//! Reading markdown: [`inline`] is a line's marks, links and dates, [`blocks`]
//! the lines' structure (lists, quotes, rules, images), and `parse` here ties them
//! together. Writing it: [`write`]. [`expressible`] says which blocks markdown
//! can't round-trip. [`doc`] walks docs.

mod blocks;
mod doc;
mod expressible;
mod inline;
#[cfg(test)]
mod tests;
mod write;

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::structures::STRUCTURES;

pub use doc::chip_ids;
pub use expressible::inexpressible;
pub use write::{block_markdown, mention_markdown};

use self::blocks::parse_blocks;
use self::doc::{drop_unresolved, walk, walk_mut};

/// The structure a bare [[Name]] refers to. Any other structure is written
/// [[Structure/Name]].
const DEFAULT_MENTION_STRUCTURE: &str = "Note";

/// A chip target's current identity, looked up by id when writing markdown.
#[derive(Debug, Clone, PartialEq)]
pub struct ChipTarget {
    pub name: String,
    pub structure_type: String,
}

/// A (structure type, name) a note's markdown refers to, to be resolved to an
/// entity before the doc can be saved.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub structure_type: String,
    pub name: String,
}

impl Reference {
    /// Names match case-insensitively, as the server's ResolveEntity does.
    pub fn key(&self) -> String {
        ref_key(&self.structure_type, &self.name)
    }
}

fn ref_key(structure_type: &str, name: &str) -> String {
    format!("{structure_type}\0{}", name.trim().to_lowercase())
}

/// Splits "Structure/Name" when Structure is exactly a registry structure type
/// and Name is non-empty; otherwise the whole text is a Note name.
fn split_structure_prefix(text: &str) -> ChipTarget {
    if let Some(slash) = text.find('/').filter(|&s| s > 0) {
        let structure_type = &text[..slash];
        let name = text[slash + 1..].trim();
        if !name.is_empty() && STRUCTURES.iter().any(|s| s.type_ == structure_type) {
            return ChipTarget {
                name: name.to_string(),
                structure_type: structure_type.to_string(),
            };
        }
    }
    ChipTarget {
        name: text.to_string(),
        structure_type: DEFAULT_MENTION_STRUCTURE.to_string(),
    }
}

/// A doc whose chips still await ids.
pub struct Parsed {
    doc: Value,
    /// Each distinct (structure, name) the markdown refers to.
    pub references: Vec<Reference>,
}

pub fn parse(markdown: &str) -> Parsed {
    let blocks = parse_blocks(markdown);
    let mut doc = json!({
        "type": "doc",
        "content": if blocks.is_empty() { vec![json!({ "type": "paragraph" })] } else { blocks },
    });

    // [[Structure/Name]]: retarget the mention when the prefix is a registry type.
    walk_mut(&mut doc, &mut |n| {
        if n["type"] != "mention" {
            return;
        }
        let Some(attrs) = n.get_mut("attrs").and_then(Value::as_object_mut) else {
            return;
        };
        let Some(raw) = attrs
            .get("_name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return;
        };
        let target = split_structure_prefix(raw);
        let aliased = attrs.remove("_aliased") == Some(Value::Bool(true));
        attrs.insert("structureType".into(), target.structure_type.into());
        attrs.insert("_name".into(), target.name.clone().into());
        // An unaliased [[Tag/ship]] is labelled "ship", not the prefixed text.
        if !aliased {
            attrs.insert("label".into(), target.name.into());
        }
    });

    let mut references: Vec<Reference> = Vec::new();
    walk(&doc, &mut |n| {
        let Some(name) = n["attrs"]["_name"].as_str().filter(|s| !s.is_empty()) else {
            return;
        };
        let reference = Reference {
            structure_type: n["attrs"]["structureType"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            name: name.to_string(),
        };
        if !references.iter().any(|r| r.key() == reference.key()) {
            references.push(reference);
        }
    });
    Parsed { doc, references }
}

impl Parsed {
    /// Stamp each chip with its entity id, keyed by `Reference::key`. A chip whose
    /// reference isn't in `ids` (a target that doesn't exist and can't be
    /// created) becomes plain text.
    pub fn resolve(mut self, ids: &HashMap<String, String>) -> Value {
        walk_mut(&mut self.doc, &mut |n| {
            let Some(attrs) = n.get_mut("attrs").and_then(Value::as_object_mut) else {
                return;
            };
            let Some(Value::String(name)) = attrs.remove("_name").filter(|v| v != "") else {
                return;
            };
            let structure_type = attrs
                .get("structureType")
                .and_then(Value::as_str)
                .unwrap_or("");
            let id = ids
                .get(&ref_key(structure_type, &name))
                .cloned()
                .unwrap_or_default();
            attrs.insert("id".into(), id.into());
        });
        drop_unresolved(&mut self.doc);
        self.doc
    }
}
