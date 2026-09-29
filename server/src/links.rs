//! Doc-walker: extract mention targets, date references and plain text from a TipTap
//! JSON doc.
//!
//! This is the only implementation of "what does this doc reference"; no client has a
//! copy. `RichTextService.PutRichText` uses it to derive a doc's content links and
//! referenced dates server-side, so every writer (browser, MCP agent, future extraction)
//! gets identical graph edges and no client authors a content link directly.

use std::collections::HashSet;

use serde_json::Value;

use crate::error::AppError;

/// A link target: an entity id. Its structure type isn't carried; the link store
/// reads the target's real `structure_type` when it writes the row (I-51).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionRef {
    pub id: String,
}

/// References extracted from a richtext doc. Entities are de-duped by id
/// (first occurrence wins); dates are de-duped preserving first-seen order.
#[derive(Debug, Default)]
pub struct DocReferences {
    pub entities: Vec<MentionRef>,
    pub dates: Vec<String>,
}

const MENTION_TYPES: [&str; 2] = ["mention", "hashtag"];

/// Whether `s` is a real calendar day in canonical `yyyy-MM-dd` form (no padding
/// left out, no surrounding space). The one date check the server makes: date
/// property values, Resolve's date key and a `dateChip`'s `date` all use it (I-52).
pub(crate) fn is_iso_day(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .is_ok_and(|d| d.format("%Y-%m-%d").to_string() == s)
}

pub fn extract_doc_references(doc_json: &str) -> Result<DocReferences, AppError> {
    let mut refs = DocReferences::default();
    if doc_json.trim().is_empty() {
        return Ok(refs);
    }
    let doc: Value = serde_json::from_str(doc_json)
        .map_err(|e| AppError::Invalid(format!("invalid richtext doc json: {e}")))?;

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut seen_dates: HashSet<String> = HashSet::new();
    walk(&doc, &mut refs, &mut seen_ids, &mut seen_dates);
    Ok(refs)
}

fn walk(
    node: &Value,
    refs: &mut DocReferences,
    seen_ids: &mut HashSet<String>,
    seen_dates: &mut HashSet<String>,
) {
    if let Some(node_type) = node.get("type").and_then(Value::as_str) {
        if MENTION_TYPES.contains(&node_type) {
            // Only `attrs.id` matters; `structureType` is ignored (I-51).
            let id = node
                .get("attrs")
                .and_then(|a| a.get("id"))
                .and_then(Value::as_str);
            if let Some(id) = id {
                if seen_ids.insert(id.to_string()) {
                    refs.entities.push(MentionRef { id: id.to_string() });
                }
            }
        } else if node_type == "dateChip" {
            if let Some(iso) = node
                .get("attrs")
                .and_then(|a| a.get("date"))
                .and_then(Value::as_str)
                .filter(|d| is_iso_day(d))
            {
                if seen_dates.insert(iso.to_string()) {
                    refs.dates.push(iso.to_string());
                }
            }
        }
    }
    if let Some(content) = node.get("content").and_then(Value::as_array) {
        for child in content {
            walk(child, refs, seen_ids, seen_dates);
        }
    }
}

/// Collect the plain-text content of a TipTap JSON doc into a single string (see
/// `collect_text` for the rule). Used to feed the FTS `body` column (M7) —
/// sibling to `extract_doc_references`, walking the same node tree. An empty /
/// blank doc yields an empty string.
pub fn extract_plain_text(doc_json: &str) -> Result<String, AppError> {
    if doc_json.trim().is_empty() {
        return Ok(String::new());
    }
    let doc: Value = serde_json::from_str(doc_json)
        .map_err(|e| AppError::Invalid(format!("invalid richtext doc json: {e}")))?;

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
    fn extracts_mentions_hashtags_and_dates_with_dedup() {
        let doc = r#"{"type":"doc","content":[
            {"type":"paragraph","content":[
                {"type":"text","text":"see "},
                {"type":"mention","attrs":{"id":"e1","structureType":"Note"}},
                {"type":"text","text":" and "},
                {"type":"hashtag","attrs":{"id":"t1","structureType":"Tag"}},
                {"type":"dateChip","attrs":{"date":"2026-06-15"}},
                {"type":"mention","attrs":{"id":"e1","structureType":"Note"}}
            ]}
        ]}"#;
        let refs = extract_doc_references(doc).unwrap();
        assert_eq!(refs.entities.len(), 2, "e1 deduped, t1 kept");
        assert_eq!(refs.entities[0].id, "e1");
        assert_eq!(refs.entities[1].id, "t1");
        assert_eq!(refs.dates, vec!["2026-06-15".to_string()]);
    }

    // I-51: a mention needs only an id; its structureType is ignored.
    #[test]
    fn mentions_without_a_string_structure_type_still_count() {
        let doc = r#"{"type":"doc","content":[
            {"type":"paragraph","content":[
                {"type":"mention","attrs":{"id":"e1","structureType":null}},
                {"type":"hashtag","attrs":{"id":"t1"}},
                {"type":"mention","attrs":{"id":null,"structureType":"Note"}},
                {"type":"mention","attrs":{}}
            ]}
        ]}"#;
        let refs = extract_doc_references(doc).unwrap();
        let ids: Vec<&str> = refs.entities.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["e1", "t1"]);
    }

    // I-52: a chip whose date isn't an ISO day is skipped, as a missing date is.
    #[test]
    fn date_chips_with_a_non_iso_date_are_skipped() {
        let doc = r#"{"type":"doc","content":[
            {"type":"paragraph","content":[
                {"type":"dateChip","attrs":{"date":"2026-13-45"}},
                {"type":"dateChip","attrs":{"date":"2026-02-03"}},
                {"type":"dateChip","attrs":{"date":"2026-2-3"}},
                {"type":"dateChip","attrs":{"date":"June 13"}},
                {"type":"dateChip","attrs":{"date":""}},
                {"type":"dateChip","attrs":{}}
            ]}
        ]}"#;
        let refs = extract_doc_references(doc).unwrap();
        assert_eq!(refs.dates, vec!["2026-02-03".to_string()]);
    }

    #[test]
    fn is_iso_day_accepts_only_canonical_real_days() {
        for good in ["2026-02-03", "2024-02-29"] {
            assert!(is_iso_day(good), "{good}");
        }
        for bad in [
            "2026-13-45",
            "2026-2-3",
            "2025-02-29",
            "June 13",
            "",
            " 2026-02-03",
        ] {
            assert!(!is_iso_day(bad), "{bad:?}");
        }
    }

    #[test]
    fn empty_doc_is_ok() {
        assert!(extract_doc_references("").unwrap().entities.is_empty());
        assert!(extract_doc_references("   ").unwrap().dates.is_empty());
    }

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
