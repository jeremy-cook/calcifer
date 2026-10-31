use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;
use tonic::Code;

use super::doc::doc_ref;
use super::lines::{block_range, is_empty_paragraph, refuse_inexpressible, BlockSpan};
use super::{blocks, fail, is_code, timestamp_ms, Knowledge, NoteKey, ToolError, ToolResult};

pub(super) static FRONTMATTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^---\r?\n(?:[A-Za-z_][\w-]*:.*\r?\n)+---[ \t]*(?:\r?\n|$)").unwrap()
});

/// A leading `---` block of `key: value` lines, as get_note writes it. Anything
/// else starting with `---` is note content and is left alone.
pub(super) fn strip_frontmatter(markdown: &str) -> &str {
    FRONTMATTER
        .find(markdown)
        .map_or(markdown, |m| &markdown[m.end()..])
}

/// The `tags:` line of a leading frontmatter block, as get_note writes it: a
/// JSON list of tag names. `None` when there is no block or it has no such line.
/// A value that isn't a list of strings is an error, so a copied note never
/// loses its tags without saying so.
pub(super) fn frontmatter_tags(markdown: &str) -> Result<Option<Vec<String>>, ToolError> {
    let Some(block) = FRONTMATTER.find(markdown) else {
        return Ok(None);
    };
    let Some(value) = block
        .as_str()
        .lines()
        .find_map(|line| line.strip_prefix("tags:"))
    else {
        return Ok(None);
    };
    match serde_json::from_str::<Vec<String>>(value.trim()) {
        Ok(tags) => Ok(Some(tags)),
        Err(_) => fail(format!(
            "The frontmatter line `tags:{value}` must be a JSON list of tag names, like tags: [\"ml\",\"papers\"]. Nothing was written."
        )),
    }
}

pub(super) const MAX_FOUND_BLOCKS: usize = 20;

impl Knowledge {
    /// Where `query` occurs in a note's body, as the whole-block line ranges
    /// edit_note takes (a match inside a list item reports the whole list), with
    /// the version those lines belong to. The match is exact and case-sensitive
    /// against the markdown get_note shows, so the agent quotes text it has read
    /// instead of counting lines.
    pub async fn query_text_location(&self, key: NoteKey<'_>, query: &str) -> ToolResult {
        if query.is_empty() {
            return fail("query is empty: say which text to find.");
        }
        let found = self.existing_doc(&key).await?;
        let (doc, updated_at) = self
            .get_doc(&doc_ref(&found.structure_type, &found.id)?)
            .await?;
        let (body, spans) = self.render_body(&doc).await?;
        let line_at = |offset: usize| body[..offset].matches('\n').count() + 1;

        // Each block that holds a match, with how many matches fall in it. A match
        // that spans blocks reports each block it touches.
        let mut hits: Vec<(BlockSpan, usize)> = Vec::new();
        let mut from = 0;
        while let Some(found) = body[from..].find(query) {
            let at = from + found;
            let end = at + query.len();
            let last_char = body[at..end].chars().next_back().map_or(0, char::len_utf8);
            let (first, last) = (line_at(at), line_at(end - last_char));
            for span in spans.iter().filter(|s| s.last >= first && s.first <= last) {
                match hits.iter_mut().find(|(s, _)| s.index == span.index) {
                    Some((_, n)) => *n += 1,
                    None => hits.push((*span, 1)),
                }
            }
            from = end;
        }
        if hits.is_empty() {
            return Ok(format!(
                "No match for {} in {}.",
                json!(query),
                found.label()
            ));
        }

        let lines: Vec<&str> = body.split('\n').collect();
        let shown: Vec<String> = hits
            .iter()
            .take(MAX_FOUND_BLOCKS)
            .map(|(s, n)| {
                let matches = if *n == 1 {
                    "1 match".to_string()
                } else {
                    format!("{n} matches")
                };
                format!(
                    "lines {}-{} ({matches}):\n{}",
                    s.first,
                    s.last,
                    lines[s.first - 1..s.last].join("\n")
                )
            })
            .collect();
        let more = if hits.len() > shown.len() {
            format!(
                "\n\n({} more block(s) not shown; use a longer query.)",
                hits.len() - shown.len()
            )
        } else {
            String::new()
        };
        Ok(format!(
            "version: {}\n\n{}{more}",
            timestamp_ms(&updated_at),
            shown.join("\n\n")
        ))
    }

    /// Replace lines `start..=end` (1-based) of a note's body with `markdown`. An
    /// empty range (end = start - 1) inserts before line `start`; empty markdown
    /// deletes the range. `version` is the one get_note put in the frontmatter:
    /// line numbers only mean something against the document they were read
    /// from, so a changed note is an error, not a retry.
    pub async fn edit_note(
        &self,
        key: NoteKey<'_>,
        start: usize,
        end: usize,
        markdown: &str,
        version: i64,
    ) -> ToolResult {
        if start == 0 || version < 0 {
            return fail("start must be at least 1 and version at least 0; nothing was written.");
        }
        let found = self.existing_doc(&key).await?;
        let stale = || {
            ToolError(format!(
                "{} changed since you read it. Call get_note again and redo the edit.",
                found.label()
            ))
        };

        let r = doc_ref(&found.structure_type, &found.id)?;
        let (doc, updated_at) = self.get_doc(&r).await?;
        if timestamp_ms(&updated_at) != version {
            return Err(stale());
        }

        let (_, spans) = self.render_body(&doc).await?;
        let mut existing = blocks(&doc);
        // In a note with no visible block, an insert shouldn't land beside empty
        // paragraphs. Anything else unseen stays: it's content all the same.
        if spans.is_empty() {
            existing.retain(|b| !is_empty_paragraph(b));
        }
        let (from, to) = block_range(&spans, existing.len(), start, end)?;
        refuse_inexpressible(&spans, &existing, from, to)?;

        let replacement = if markdown.trim().is_empty() {
            Vec::new()
        } else {
            blocks(&self.parse_markdown(markdown).await?)
        };
        let mut content = existing[..from].to_vec();
        content.extend(replacement.iter().cloned());
        content.extend(existing[to..].iter().cloned());
        // An empty doc isn't valid TipTap; a note whose blocks were all deleted keeps one empty paragraph.
        if content.is_empty() {
            content.push(json!({ "type": "paragraph" }));
        }
        let merged = json!({ "type": "doc", "content": content });
        match self.put_doc(&r, &merged, Some(updated_at)).await {
            Ok(()) => {}
            Err(e) if is_code(&e, Code::FailedPrecondition) => return Err(stale()),
            Err(e) => return Err(e.into()),
        }

        let ent = self.entity(&found.id).await?;
        let action = if end + 1 == start {
            format!("Inserted at line {start}")
        } else if replacement.is_empty() {
            format!("Deleted lines {start}-{end}")
        } else {
            format!("Replaced lines {start}-{end}")
        };
        Ok(format!(
            "{action} of {} ({}); now {} wikilink(s).",
            found.label(),
            found.id,
            r.link_count(&ent)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_is_stripped_only_when_it_is_key_value_lines() {
        assert_eq!(
            strip_frontmatter("---\nname: \"x\"\nversion: 3\n---\nBody"),
            "Body"
        );
        assert_eq!(
            strip_frontmatter("---\nplain text\n---"),
            "---\nplain text\n---"
        );
        assert_eq!(strip_frontmatter("Body"), "Body");
    }
}
