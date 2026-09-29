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

/// A mention extracted from a doc: the target entity id and its structure type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionRef {
    pub id: String,
    pub structure_type: String,
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
            let attrs = node.get("attrs");
            let id = attrs.and_then(|a| a.get("id")).and_then(Value::as_str);
            let structure_type = attrs
                .and_then(|a| a.get("structureType"))
                .and_then(Value::as_str);
            if let (Some(id), Some(structure_type)) = (id, structure_type) {
                if seen_ids.insert(id.to_string()) {
                    refs.entities.push(MentionRef {
                        id: id.to_string(),
                        structure_type: structure_type.to_string(),
                    });
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

/// Collect the plain-text content of a TipTap JSON doc into a single string,
/// space-joining text nodes across blocks. Used to feed the FTS `body` column
/// (M7) — sibling to `extract_doc_references`, walking the same node tree.
/// An empty / blank doc yields an empty string.
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

fn collect_text(node: &Value, out: &mut String) {
    if let Some(text) = node.get("text").and_then(Value::as_str) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(text);
    }
    if let Some(content) = node.get("content").and_then(Value::as_array) {
        for child in content {
            collect_text(child, out);
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
        assert_eq!(refs.entities[1].structure_type, "Tag");
        assert_eq!(refs.dates, vec!["2026-06-15".to_string()]);
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
                {"type":"text","text":"Hello"},
                {"type":"mention","attrs":{"id":"e1","structureType":"Note"}},
                {"type":"text","text":"world"}
            ]},
            {"type":"paragraph","content":[{"type":"text","text":"second"}]}
        ]}"#;
        assert_eq!(extract_plain_text(doc).unwrap(), "Hello world second");
    }

    #[test]
    fn empty_doc_yields_empty_text() {
        assert_eq!(extract_plain_text("").unwrap(), "");
        assert_eq!(extract_plain_text("   ").unwrap(), "");
    }
}
