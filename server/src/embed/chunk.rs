//! Chunk an entity's TipTap JSON docs into ~500-token text slices for embedding.
//!
//! A header (the entity's name and Tags, from `search_index`) is the first
//! chunk, on its own (I-87): leading the first body chunk with it instead lost
//! 4 of 29 top-1 semantic hits on the Q1 scorecard, since a note's distinctive
//! text then shares its vector with labels many notes carry. Then we walk each doc's top-level blocks (paragraphs, headings, list items, …),
//! flatten each block to plain text with `tiptap::text::collect_text`, the same rule
//! `extract_plain_text` uses for FTS (I-54), then greedily pack whole blocks into
//! chunks until adding the next block would exceed the token budget. Blocks are never split mid-way (a single
//! oversized block becomes its own chunk) so a chunk stays a coherent unit.
//!
//! "Tokens" here are approximated by whitespace-delimited words — close enough to
//! keep MiniLM's 256-wordpiece window comfortably in range without pulling in a
//! tokenizer. Empty/blank blocks are dropped.

use serde_json::Value;

use crate::tiptap::text::{collect_text, ChipNames};

/// Target chunk size in approximate tokens (whitespace words). MiniLM truncates
/// past ~256 wordpieces; 500 words sits near that once subword expansion is
/// accounted for, and fastembed truncates the overflow rather than erroring.
const CHUNK_TARGET_TOKENS: usize = 500;

/// Split `header` and the blocks of `docs` into ordered chunk texts: the
/// header alone, unless blank, then the docs' blocks packed, a chip reading as
/// its target's name in `names`. A blank, empty or invalid doc adds nothing.
pub fn chunk_docs(header: &str, docs: &[String], names: &ChipNames) -> Vec<String> {
    let header = header.trim();
    let mut blocks = Vec::new();
    for doc_json in docs {
        if let Ok(doc) = serde_json::from_str::<Value>(doc_json) {
            blocks.extend(block_texts(&doc, names));
        }
    }
    let mut chunks = Vec::new();
    if !header.is_empty() {
        chunks.push(header.to_string());
    }
    chunks.extend(pack(blocks));
    chunks
}

/// Flatten each top-level block of the doc into one plain-text string, dropping
/// blanks. Falls back to treating the whole doc as one block when it has no
/// `content` array.
fn block_texts(doc: &Value, names: &ChipNames) -> Vec<String> {
    let Some(content) = doc.get("content").and_then(Value::as_array) else {
        let mut s = String::new();
        collect_text(doc, names, &mut s);
        let s = s.trim().to_string();
        return if s.is_empty() { vec![] } else { vec![s] };
    };

    content
        .iter()
        .filter_map(|block| {
            let mut s = String::new();
            collect_text(block, names, &mut s);
            let s = s.trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .collect()
}

/// Greedily pack whole blocks into chunks of up to CHUNK_TARGET_TOKENS words.
fn pack(blocks: Vec<String>) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_tokens = 0usize;

    for block in blocks {
        let block_tokens = block.split_whitespace().count();
        if !cur.is_empty() && cur_tokens + block_tokens > CHUNK_TARGET_TOKENS {
            chunks.push(std::mem::take(&mut cur));
            cur_tokens = 0;
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(&block);
        cur_tokens += block_tokens;
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chunks of one doc with no header.
    fn chunk_doc(doc: &str) -> Vec<String> {
        chunk_docs("", &[doc.to_string()], &ChipNames::new())
    }

    #[test]
    fn empty_doc_yields_no_chunks() {
        assert!(chunk_doc("").is_empty());
        assert!(chunk_doc("   ").is_empty());
        assert!(chunk_doc("{not json").is_empty());
    }

    // I-87: the header is the first chunk, on its own.
    #[test]
    fn the_header_is_the_first_chunk() {
        let doc = r#"{"type":"doc","content":[
            {"type":"paragraph","content":[{"type":"text","text":"hello world"}]}
        ]}"#;
        let names = ChipNames::new();
        assert_eq!(
            chunk_docs(
                "Apollo 11\nTags: program.apollo",
                &[doc.to_string()],
                &names
            ),
            vec![
                "Apollo 11\nTags: program.apollo".to_string(),
                "hello world".to_string()
            ]
        );
        assert_eq!(
            chunk_docs("Apollo 11", &[], &names),
            vec!["Apollo 11".to_string()]
        );
    }

    #[test]
    fn small_doc_packs_into_one_chunk() {
        let doc = r#"{"type":"doc","content":[
            {"type":"heading","content":[{"type":"text","text":"Title"}]},
            {"type":"paragraph","content":[{"type":"text","text":"hello world"}]}
        ]}"#;
        let chunks = chunk_doc(doc);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Title\nhello world");
    }

    #[test]
    fn large_doc_splits_across_chunks_on_block_boundaries() {
        // Two blocks each ~400 tokens => exceed 500 together => two chunks.
        let big = (0..400).map(|_| "word").collect::<Vec<_>>().join(" ");
        let doc = format!(
            r#"{{"type":"doc","content":[
                {{"type":"paragraph","content":[{{"type":"text","text":"{big}"}}]}},
                {{"type":"paragraph","content":[{{"type":"text","text":"{big}"}}]}}
            ]}}"#
        );
        let chunks = chunk_doc(&doc);
        assert_eq!(
            chunks.len(),
            2,
            "two ~400-token blocks split into two chunks"
        );
    }

    // I-54: a chunk follows the FTS text rule inside each block.
    #[test]
    fn chunk_text_joins_inline_nodes_and_includes_chip_text() {
        let doc = r#"{"type":"doc","content":[
            {"type":"paragraph","content":[
                {"type":"text","marks":[{"type":"italic"}],"text":"bold"},
                {"type":"text","text":"er with "},
                {"type":"mention","attrs":{"id":"e1","label":"Ada"}},
                {"type":"hashtag","attrs":{"id":"t1","label":"math"}},
                {"type":"hardBreak"},
                {"type":"dateChip","attrs":{"date":"2026-06-15"}}
            ]},
            {"type":"paragraph","content":[{"type":"text","text":"next"}]}
        ]}"#;
        assert_eq!(
            chunk_doc(doc),
            vec!["bolder with Adamath 2026-06-15\nnext".to_string()]
        );
    }
}
