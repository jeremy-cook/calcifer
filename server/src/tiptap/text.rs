//! A doc as plain text: the one rule for the FTS `body` column and for
//! embedding chunks (I-54).

use serde_json::Value;

use super::{parse, MENTION_TYPES};
use crate::error::AppError;

/// Collect the plain-text content of a TipTap JSON doc into a single string (see
/// `collect_text` for the rule). Used to feed the FTS `body` column (M7) —
/// sibling to `references::extract_doc_references`, walking the same node tree. An empty /
/// blank doc yields an empty string.
pub fn extract_plain_text(doc_json: &str) -> Result<String, AppError> {
    let Some(doc) = parse(doc_json)? else {
        return Ok(String::new());
    };
    let mut out = String::new();
    collect_text(&doc, &mut out);
    Ok(out.trim().to_string())
}

/// Append the plain text under `node` to `out`: the one text rule for FTS and
/// embedding chunks (I-54). Inline nodes join with no separator: a `text` node
/// gives its `text`, a `mention`/`hashtag` its `attrs.label` and a `dateChip`
/// its `attrs.date` (each only if a string), and a `hardBreak` gives a space.
/// Any other node with `content` is a block, set off from its neighbours by one
/// space. Labels are the snapshot stored in the doc, so they can be stale after
/// a rename. The result can carry a leading or trailing space; callers trim.
pub(crate) fn collect_text(node: &Value, out: &mut String) {
    let node_type = node.get("type").and_then(Value::as_str);
    let attr = |key: &str| {
        node.get("attrs")
            .and_then(|a| a.get(key))
            .and_then(Value::as_str)
    };
    match node_type {
        Some("text") => {
            if let Some(text) = node.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
        Some(t) if MENTION_TYPES.contains(&t) => out.push_str(attr("label").unwrap_or_default()),
        Some("dateChip") => out.push_str(attr("date").unwrap_or_default()),
        Some("hardBreak") => out.push(' '),
        _ => {
            let Some(content) = node.get("content").and_then(Value::as_array) else {
                return;
            };
            block_boundary(out);
            for child in content {
                collect_text(child, out);
            }
            block_boundary(out);
        }
    }
}

/// Separate a block from the text before or after it with one space.
fn block_boundary(out: &mut String) {
    if !out.is_empty() && !out.ends_with(' ') {
        out.push(' ');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_plain_text_across_blocks() {
        let doc = r#"{"type":"doc","content":[
            {"type":"paragraph","content":[
                {"type":"text","text":"Hello "},
                {"type":"mention","attrs":{"id":"e1","structureType":"Note"}},
                {"type":"text","text":"world"}
            ]},
            {"type":"paragraph","content":[{"type":"text","text":"second"}]},
            {"type":"bulletList","content":[
                {"type":"listItem","content":[
                    {"type":"paragraph","content":[{"type":"text","text":"item"}]}
                ]}
            ]}
        ]}"#;
        assert_eq!(extract_plain_text(doc).unwrap(), "Hello world second item");
    }

    // I-54: inline nodes join without a separator, and chips give their text.
    #[test]
    fn plain_text_joins_inline_nodes_and_includes_chip_text() {
        let doc = r#"{"type":"doc","content":[
            {"type":"paragraph","content":[
                {"type":"text","marks":[{"type":"bold"}],"text":"bold"},
                {"type":"text","text":"er, see "},
                {"type":"mention","attrs":{"id":"e1","label":"Ada Lovelace"}},
                {"type":"text","text":" and "},
                {"type":"hashtag","attrs":{"id":"t1","label":"history"}},
                {"type":"hardBreak"},
                {"type":"text","text":"on "},
                {"type":"dateChip","attrs":{"date":"2026-06-15"}},
                {"type":"mention","attrs":{"id":"e2","label":null}}
            ]},
            {"type":"paragraph","content":[{"type":"text","text":"next"}]}
        ]}"#;
        assert_eq!(
            extract_plain_text(doc).unwrap(),
            "bolder, see Ada Lovelace and history on 2026-06-15 next"
        );
    }

    #[test]
    fn empty_doc_yields_empty_text() {
        assert_eq!(extract_plain_text("").unwrap(), "");
        assert_eq!(extract_plain_text("   ").unwrap(), "");
    }
}
