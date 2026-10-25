use serde_json::Value;

use super::inline::{date_at, Mark};
use super::write::{image_markdown, is_list};

/// An image whose markdown carries all of it: the editor's caption, width and
/// alignment have no markdown form.
pub(super) fn image_is_expressible(attrs: &Value) -> bool {
    image_markdown(attrs).is_some()
        && attrs["caption"].as_str().is_none_or(str::is_empty)
        && attrs["width"].is_null()
        && attrs["alignment"].as_str().is_none_or(|a| a == "left")
}

/// `noun` after "a" or "an".
pub(super) fn with_article(noun: &str) -> String {
    let article = if noun.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {noun}")
}

/// Whether markdown carries attribute `key` of a `node_type` node. Any attribute
/// not listed here must hold its default, which for every such attribute the
/// editor defines (`textAlign`, `orderedList.type`, `image.height`) is null.
pub(super) fn attr_fits(node_type: &str, key: &str, value: &Value) -> bool {
    match (node_type, key) {
        ("heading", "level") => value.as_u64().is_some_and(|l| (1..=6).contains(&l)),
        ("orderedList", "start") => value.is_u64(),
        ("taskItem", "checked") => value.is_boolean(),
        ("codeBlock", "language") => {
            value.is_null()
                || value.as_str().is_some_and(|l| {
                    !l.is_empty() && !l.contains(char::is_whitespace) && !l.contains('`')
                })
        }
        ("dateChip", "date") => value.as_str().is_some_and(|d| date_at(d) == Some(d)),
        // A chip is written from its target, whatever its stored label says; an
        // image's attributes are checked by image_is_expressible.
        (
            "mention" | "hashtag",
            "id" | "label" | "structureType" | "char" | "mentionSuggestionChar",
        )
        | ("image", "src" | "alt" | "title" | "caption" | "width" | "alignment") => true,
        _ => value.is_null(),
    }
}

/// What markdown would drop of `node`'s own marks: a mark it doesn't write,
/// a link it can't, or code it can't (code is verbatim and stands alone).
pub(super) fn inexpressible_marks(node: &Value) -> Option<String> {
    let marks = node["marks"].as_array().filter(|ms| !ms.is_empty())?;
    let node_type = node["type"].as_str().unwrap_or("node");
    if node_type != "text" {
        return Some(format!("formatting on {}", with_article(node_type)));
    }
    for mark in marks {
        let mark_type = mark["type"].as_str().unwrap_or("unknown");
        if Mark::from_json(mark).is_none() {
            return Some(if mark_type == "link" {
                "a link".into()
            } else {
                format!("{mark_type} formatting")
            });
        }
    }
    let code = marks.iter().any(|m| m["type"] == "code");
    if code && marks.len() > 1 {
        return Some("code combined with other formatting".into());
    }
    if code && node["text"].as_str().is_some_and(|t| t.contains('`')) {
        return Some("inline code containing a backtick".into());
    }
    None
}

/// What replacing `block` from its markdown would lose, phrased to follow
/// "lines 3-4 hold": the first node in it (itself included) that this converter
/// doesn't know, or that sits in a shape markdown doesn't have (a list in a
/// quote, two paragraphs in a list item, ...), or that carries an attribute or
/// mark markdown can't write.
pub fn inexpressible(block: &Value) -> Option<String> {
    let node_type = block["type"].as_str().unwrap_or("node");
    let children = block["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let fits = match node_type {
        "image" => image_is_expressible(&block["attrs"]),
        "blockquote" => children.iter().all(|c| c["type"] == "paragraph"),
        "listItem" | "taskItem" => {
            children.first().is_some_and(|c| c["type"] == "paragraph")
                && children.iter().skip(1).all(is_list)
        }
        "heading" => children.iter().all(|c| c["type"] != "hardBreak"),
        "codeBlock" => {
            let code = block["content"][0]["text"].as_str().unwrap_or("");
            children.len() <= 1 && !code.split('\n').any(|l| l.starts_with("```"))
        }
        _ => KNOWN_NODES.contains(&node_type),
    };
    if !fits {
        return Some(with_article(node_type));
    }
    let mut attrs = block["attrs"].as_object().into_iter().flatten();
    if let Some((key, _)) = attrs.find(|(key, value)| !attr_fits(node_type, key, value)) {
        return Some(format!("{}'s {key}", with_article(node_type)));
    }
    inexpressible_marks(block).or_else(|| children.iter().find_map(inexpressible))
}

/// The nodes this converter writes and reads back.
pub(super) const KNOWN_NODES: [&str; 16] = [
    "paragraph",
    "heading",
    "text",
    "mention",
    "hashtag",
    "dateChip",
    "hardBreak",
    "bulletList",
    "orderedList",
    "listItem",
    "taskList",
    "taskItem",
    "blockquote",
    "codeBlock",
    "horizontalRule",
    "image",
];
