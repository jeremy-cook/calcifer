//! Chunk a TipTap JSON doc into ~500-token text slices for embedding.
//!
//! We walk the doc's top-level blocks (paragraphs, headings, list items, …),
//! flatten each block to plain text via the same node walker `extract_plain_text`
//! uses, then greedily pack whole blocks into chunks until adding the next block
//! would exceed the token budget. Blocks are never split mid-way (a single
//! oversized block becomes its own chunk) so a chunk stays a coherent unit.
//!
//! "Tokens" here are approximated by whitespace-delimited words — close enough to
//! keep MiniLM's 256-wordpiece window comfortably in range without pulling in a
//! tokenizer. Empty/blank blocks are dropped.

use serde_json::Value;

/// Target chunk size in approximate tokens (whitespace words). MiniLM truncates
/// past ~256 wordpieces; 500 words sits near that once subword expansion is
/// accounted for, and fastembed truncates the overflow rather than erroring.
const CHUNK_TARGET_TOKENS: usize = 500;

/// Split a TipTap doc into ordered chunk texts. Returns an empty vec for a
/// blank/empty/invalid doc (the worker treats that as "this entity has no
/// embeddable content" and clears its rows).
pub fn chunk_doc(doc_json: &str) -> Vec<String> {
    if doc_json.trim().is_empty() {
        return vec![];
    }
    let doc: Value = match serde_json::from_str(doc_json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let blocks = block_texts(&doc);
    pack(blocks)
}

/// Flatten each top-level block of the doc into one plain-text string, dropping
/// blanks. Falls back to treating the whole doc as one block when it has no
/// `content` array.
fn block_texts(doc: &Value) -> Vec<String> {
    let Some(content) = doc.get("content").and_then(Value::as_array) else {
        let mut s = String::new();
        collect_text(doc, &mut s);
        let s = s.trim().to_string();
        return if s.is_empty() { vec![] } else { vec![s] };
    };

    content
        .iter()
        .filter_map(|block| {
            let mut s = String::new();
            collect_text(block, &mut s);
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

/// Concatenate the text nodes under `node` (mirrors links::collect_text, kept
/// local so chunking owns its own walk and stays decoupled).
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
    fn empty_doc_yields_no_chunks() {
        assert!(chunk_doc("").is_empty());
        assert!(chunk_doc("   ").is_empty());
        assert!(chunk_doc("{not json").is_empty());
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
}
