//! What a doc references: mention targets and date chips.
//!
//! `RichTextService.PutRichText` uses it to derive a doc's content links and
//! referenced dates server-side, so every writer (browser, MCP agent, future extraction)
//! gets identical graph edges and no client authors a content link directly (ADR 3).

use std::collections::HashSet;

use serde_json::Value;

use super::{parse, MENTION_TYPES};
use crate::dates::is_iso_day;
use crate::error::AppError;

/// A link target: an entity id. Its structure type isn't carried;
/// `links::store::live_targets` reads the target's real `structure_type` (I-51).
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

pub fn extract_doc_references(doc_json: &str) -> Result<DocReferences, AppError> {
    let mut refs = DocReferences::default();
    let Some(doc) = parse(doc_json)? else {
        return Ok(refs);
    };
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
    fn empty_doc_is_ok() {
        assert!(extract_doc_references("").unwrap().entities.is_empty());
        assert!(extract_doc_references("   ").unwrap().dates.is_empty());
    }
}
