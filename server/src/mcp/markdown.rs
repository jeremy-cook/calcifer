//! Markdown <-> TipTap JSON, for the agent's tools. Agents write markdown with
//! [[wikilinks]], #tags and ISO dates; `parse` emits editor-faithful
//! mention/hashtag/dateChip nodes whose ids the caller resolves by name, so
//! `RichTextService.PutRichText` derives the graph. `block_markdown` goes the
//! other way, writing each chip from its target's current name.
//!
//! [[Name]] and [[Name|label]] mention a Note; [[Structure/Name]] mentions
//! another structure when Structure is exactly a registry type (e.g.
//! [[Todo/Ship]]). Emphasis (`**bold**`, `*italic*`), `~~strike~~`, inline
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

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::structures::STRUCTURES;

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

// ---------------------------------------------------------------- markdown -> doc

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#([\p{L}\p{N}_-]+)").unwrap());
static ISO_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]{4})-([0-9]{2})-([0-9]{2})").unwrap());
static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.*)$").unwrap());
/// A list item's marker: `- ` or `* ` (a task item when `[ ]` or `[x]`
/// follows), or `N. `.
static LIST_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[-*]\s+(?:\[([ xX])\](?:\s+|$))?|([0-9]+)\.\s+)").unwrap());
static SPECIAL_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6}\s|```|>|[-*]\s+|[0-9]+\.\s+)").unwrap());
static RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^---\s*$").unwrap());
/// `![alt](src "title")`: `\` escapes in alt and title, no space or quote in src.
static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^!\[((?:[^\]\\]|\\.)*)\]\(([^\s()<>"]+)(?:\s+"((?:[^"\\]|\\.)*)")?\)\s*$"#)
        .unwrap()
});
static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<!-- calcifer:\S+ -->\s*$").unwrap());

/// A line that opens a block of its own, so it never continues a paragraph.
fn starts_block(line: &str) -> bool {
    SPECIAL_BLOCK.is_match(line)
        || RULE.is_match(line)
        || IMAGE.is_match(line)
        || PLACEHOLDER.is_match(line)
}

/// The character a backslash at the start of `text` escapes: ASCII punctuation
/// or a digit. Before anything else a backslash is just a backslash.
fn escaped_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    if chars.next() != Some('\\') {
        return None;
    }
    chars
        .next()
        .filter(|c| c.is_ascii_punctuation() || c.is_ascii_digit())
}

/// `\X` read as the literal X.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < text.len() {
        let (c, width) = match escaped_char(&text[i..]) {
            Some(c) => (c, 1 + c.len_utf8()),
            None => {
                let c = text[i..].chars().next().expect("i < text.len()");
                (c, c.len_utf8())
            }
        };
        out.push(c);
        i += width;
    }
    out
}

/// The byte offset of the first `delim` in `text` that no backslash escapes and
/// no link holds, so `*[a **b**](u)*` closes at its last star.
fn find_delimiter(text: &str, delim: &str) -> Option<usize> {
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if rest.starts_with(delim) {
            return Some(i);
        }
        i += match (escaped_char(rest), link_at(rest)) {
            (Some(c), _) => 1 + c.len_utf8(),
            (None, Some(link)) => link.len,
            (None, None) => rest.chars().next().map_or(1, char::len_utf8),
        };
    }
    None
}

/// `[text](href)` at the start of some markdown.
struct LinkSpan<'a> {
    /// The link text's markdown.
    text: &'a str,
    href: String,
    /// The whole link's length in bytes.
    len: usize,
}

/// The link `text` opens with, if any. Its text ends at the first `](` that no
/// backslash escapes and no code span or [[wikilink]] holds, and isn't empty.
/// Its href reads `\X` as X, has no whitespace, `<` or `>`, and ends at the `)`
/// that balances its parentheses.
fn link_at(text: &str) -> Option<LinkSpan<'_>> {
    let after = text.strip_prefix('[')?;
    let mut end = 0;
    while !after[end..].starts_with("](") {
        let rest = &after[end..];
        end += if let Some(c) = escaped_char(rest) {
            1 + c.len_utf8()
        } else if let Some(close) = rest
            .strip_prefix('`')
            .and_then(|r| r.find('`'))
            .filter(|&e| e > 0)
        {
            close + 2
        } else if let Some(close) = rest.strip_prefix("[[").and_then(|r| r.find("]]")) {
            close + 4
        } else {
            rest.chars().next()?.len_utf8()
        };
    }
    let dest = &after[end + 2..];
    let (mut href, mut depth, mut at) = (String::new(), 0, 0);
    loop {
        let rest = &dest[at..];
        if let Some(c) = escaped_char(rest) {
            href.push(c);
            at += 1 + c.len_utf8();
            continue;
        }
        let c = rest.chars().next()?;
        match c {
            ')' if depth == 0 => break,
            ')' => depth -= 1,
            '(' => depth += 1,
            '<' | '>' => return None,
            c if c.is_whitespace() => return None,
            _ => {}
        }
        href.push(c);
        at += c.len_utf8();
    }
    let len = 1 + end + 2 + at + 1;
    (end > 0 && !href.is_empty()).then(|| LinkSpan {
        text: &after[..end],
        href,
        len,
    })
}

fn image_node(m: &regex::Captures) -> Value {
    let attr = |i: usize| {
        m.get(i)
            .map(|t| unescape(t.as_str()))
            .filter(|t| !t.is_empty())
    };
    json!({ "type": "image", "attrs": { "src": &m[2], "alt": attr(1), "title": attr(3) } })
}

/// A mark markdown can write. Variants sit in the order the editor's schema
/// ranks them, which is the order it stores them on a text node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Mark {
    Link(String),
    Bold,
    Code,
    Italic,
    Strike,
}

/// The attributes the editor gives every link (`Link.configure` in
/// TiptapEditor.tsx); a link with any other values has no markdown form.
const LINK_TARGET: &str = "_blank";
const LINK_REL: &str = "noopener noreferrer";

impl Mark {
    fn to_json(&self) -> Value {
        match self {
            Mark::Link(href) => json!({
                "type": "link",
                "attrs": { "href": href, "target": LINK_TARGET, "rel": LINK_REL, "class": null, "title": null },
            }),
            Mark::Bold => json!({ "type": "bold" }),
            Mark::Code => json!({ "type": "code" }),
            Mark::Italic => json!({ "type": "italic" }),
            Mark::Strike => json!({ "type": "strike" }),
        }
    }

    /// The stored mark as markdown writes it; `None` for one it can't write.
    fn from_json(mark: &Value) -> Option<Mark> {
        match mark["type"].as_str()? {
            "link" => link_href(&mark["attrs"]).map(|href| Mark::Link(href.to_string())),
            "bold" => Some(Mark::Bold),
            "code" => Some(Mark::Code),
            "italic" => Some(Mark::Italic),
            "strike" => Some(Mark::Strike),
            _ => None,
        }
    }
}

/// A link's href, when it can sit in `(...)` and its other attributes are the
/// editor's defaults (a missing one reads as its default).
fn link_href(attrs: &Value) -> Option<&str> {
    let href = attrs["href"].as_str().filter(|h| !h.is_empty())?;
    let mut depth = 0;
    for c in href.chars() {
        depth += match c {
            '(' => 1,
            ')' => -1,
            '<' | '>' => return None,
            c if c.is_whitespace() => return None,
            _ => 0,
        };
        if depth < 0 {
            return None;
        }
    }
    let defaults = attrs
        .as_object()
        .into_iter()
        .flatten()
        .all(|(key, value)| match key.as_str() {
            "href" => true,
            "target" => value == LINK_TARGET,
            "rel" => value == LINK_REL,
            _ => value.is_null(),
        });
    (depth == 0 && defaults).then_some(href)
}

fn mk_text(text: &str, marks: &[Mark]) -> Value {
    let mut node = json!({ "type": "text", "text": text });
    if !marks.is_empty() {
        node["marks"] = marks.iter().map(Mark::to_json).collect();
    }
    node
}

/// "yyyy-MM-dd" naming a real day: 2026-02-28 is one, 2026-02-30 and 2026-13-45 aren't.
fn is_calendar_day(year: &str, month: &str, day: &str) -> bool {
    match (year.parse(), month.parse(), day.parse()) {
        (Ok(y), Ok(m), Ok(d)) => chrono::NaiveDate::from_ymd_opt(y, m, d).is_some(),
        _ => false,
    }
}

/// The ISO date `text` opens with, when it names a real day and no digit
/// follows (2026-06-155 is not a date): at a word boundary, a dateChip.
fn date_at(text: &str) -> Option<&str> {
    let m = ISO_DATE.captures(text)?;
    let date = m.get(0)?.as_str();
    let glued = text[date.len()..].starts_with(|c: char| c.is_ascii_digit());
    (!glued && is_calendar_day(&m[1], &m[2], &m[3])).then_some(date)
}

/// Splits a text run into marked text, mention, hashtag, dateChip and hardBreak
/// nodes; a `\n` is a hard break (see `join_lines`). mention and hashtag carry a
/// temporary `_name` attr that `Parsed::resolve` turns into an id. `\X` is a
/// literal X. Inline code is verbatim: no chips, escapes or emphasis inside it.
/// A link's text is parsed like any other run, but holds no link of its own.
fn parse_inline(text: &str, marks: &[Mark]) -> Vec<Value> {
    let mut nodes = Vec::new();
    let mut buf = String::new();
    let flush = |buf: &mut String, nodes: &mut Vec<Value>| {
        if !buf.is_empty() {
            nodes.push(mk_text(buf, marks));
            buf.clear();
        }
    };
    let in_code = marks.contains(&Mark::Code);
    let in_link = marks.iter().any(|m| matches!(m, Mark::Link(_)));

    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let at_boundary = i == 0
            || text[..i]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);

        if let Some(c) = escaped_char(rest) {
            buf.push(c);
            i += 1 + c.len_utf8();
            continue;
        }
        if rest.starts_with('\n') {
            flush(&mut buf, &mut nodes);
            nodes.push(json!({ "type": "hardBreak" }));
            i += 1;
            continue;
        }
        if let Some(after) = rest.strip_prefix("[[") {
            if let Some(end) = after.find("]]") {
                let inner = &after[..end];
                let pipe = inner.find('|');
                let name = pipe.map_or(inner, |p| &inner[..p]).trim();
                let label = pipe.map_or(inner, |p| &inner[p + 1..]).trim();
                if !name.is_empty() {
                    flush(&mut buf, &mut nodes);
                    nodes.push(json!({
                        "type": "mention",
                        "attrs": {
                            "id": "", "label": label, "structureType": "Note", "char": "@",
                            "_name": name, "_aliased": pipe.is_some(),
                        },
                    }));
                    i += 2 + end + 2;
                    continue;
                }
            }
        }
        if let Some(link) = link_at(rest).filter(|_| !in_code && !in_link) {
            flush(&mut buf, &mut nodes);
            nodes.extend(parse_inline(
                link.text,
                &with_mark(marks, Mark::Link(link.href)),
            ));
            i += link.len;
            continue;
        }
        if rest.starts_with('#') && at_boundary {
            if let Some(m) = TAG.captures(rest) {
                flush(&mut buf, &mut nodes);
                nodes.push(json!({
                    "type": "hashtag",
                    "attrs": { "id": "", "label": &m[1], "structureType": "Tag", "char": "#", "_name": &m[1] },
                }));
                i += m[0].len();
                continue;
            }
        }
        if let Some(date) = date_at(rest).filter(|_| at_boundary) {
            flush(&mut buf, &mut nodes);
            nodes.push(json!({ "type": "dateChip", "attrs": { "date": date } }));
            i += date.len();
            continue;
        }
        // Inline code: verbatim until the next backtick. The editor's code mark
        // excludes every other, so it stands alone even in a link or emphasis.
        if rest.starts_with('`') && !in_code {
            if let Some(end) = rest[1..].find('`').filter(|&e| e > 0) {
                flush(&mut buf, &mut nodes);
                nodes.push(mk_text(&rest[1..1 + end], &[Mark::Code]));
                i += 1 + end + 1;
                continue;
            }
        }
        // Emphasis, longest run first so a `*` of a `**` run isn't mistaken:
        // ***both***, **bold**, *italic*, ~~strike~~.
        let emphasis: [(&str, &[Mark]); 4] = [
            ("***", &[Mark::Bold, Mark::Italic]),
            ("**", &[Mark::Bold]),
            ("*", &[Mark::Italic]),
            ("~~", &[Mark::Strike]),
        ];
        let found = emphasis.iter().find_map(|&(delim, added)| {
            let after = rest.strip_prefix(delim)?;
            // A single star that's part of a `**` run isn't italic.
            if delim == "*" && after.starts_with('*') {
                return None;
            }
            let end = find_delimiter(after, delim).filter(|&e| e > 0)?;
            Some((delim, added, &after[..end]))
        });
        if let Some((delim, added, inner)) = found.filter(|_| !in_code) {
            flush(&mut buf, &mut nodes);
            let inner_marks = added
                .iter()
                .fold(marks.to_vec(), |ms, m| with_mark(&ms, m.clone()));
            nodes.extend(parse_inline(inner, &inner_marks));
            i += 2 * delim.len() + inner.len();
            continue;
        }
        let ch = rest.chars().next().expect("i < text.len()");
        buf.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut buf, &mut nodes);
    nodes
}

/// `marks` with `mark` added, kept in the editor's order.
fn with_mark(marks: &[Mark], mark: Mark) -> Vec<Mark> {
    let mut out = marks.to_vec();
    if !out.contains(&mark) {
        out.push(mark);
        out.sort();
    }
    out
}

/// A line ending in a `\` that isn't itself escaped: a hard break.
fn ends_in_break(line: &str) -> bool {
    line.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1
}

/// A block's source lines as one inline run: a line ending in a hard break
/// joins the next with `\n`, any other line break is a space. A `\` ending the
/// last line breaks nothing and stays literal.
fn join_lines(lines: &[&str]) -> String {
    let mut out = String::new();
    for (n, line) in lines.iter().enumerate() {
        if n + 1 == lines.len() {
            out += line;
        } else if ends_in_break(line) {
            out += &line[..line.len() - 1];
            out.push('\n');
        } else {
            out += line;
            out.push(' ');
        }
    }
    out
}

fn paragraph(lines: &[&str]) -> Value {
    json!({ "type": "paragraph", "content": parse_inline(&join_lines(lines), &[]) })
}

/// `line` after exactly `indent` spaces, when something other than space follows.
fn at_indent(line: &str, indent: usize) -> Option<&str> {
    let rest = line.get(indent..)?;
    let indented = line.as_bytes()[..indent].iter().all(|&b| b == b' ');
    (indented && !rest.is_empty() && !rest.starts_with(char::is_whitespace)).then_some(rest)
}

/// A list item's first line.
struct ItemLine<'a> {
    kind: &'static str,
    text: &'a str,
    checked: bool,
    number: u64,
}

/// The item `line` opens at exactly `indent` spaces, if any.
fn item_line(line: &str, indent: usize) -> Option<ItemLine<'_>> {
    let rest = at_indent(line, indent)?;
    let m = LIST_MARK.captures(rest)?;
    let kind = match (m.get(1), m.get(2)) {
        (_, Some(_)) => "orderedList",
        (Some(_), _) => "taskList",
        _ => "bulletList",
    };
    Some(ItemLine {
        kind,
        text: &rest[m[0].len()..],
        checked: m.get(1).is_some_and(|c| c.as_str() != " "),
        number: m.get(2).and_then(|n| n.as_str().parse().ok()).unwrap_or(1),
    })
}

/// The list whose first item is `lines[*i]`, at `indent` spaces, through its
/// last item of the same kind. An item's continuation lines and nested lists
/// sit two spaces deeper.
fn parse_list(lines: &[&str], i: &mut usize, indent: usize) -> Value {
    let first = item_line(lines[*i], indent).expect("the caller found an item");
    let (kind, start) = (first.kind, first.number);
    let mut items = Vec::new();
    while let Some(item) = lines
        .get(*i)
        .and_then(|l| item_line(l, indent))
        .filter(|item| item.kind == kind)
    {
        *i += 1;
        let mut content = Vec::new();
        let mut pending = vec![item.text];
        while let Some(deeper) = lines.get(*i).and_then(|l| at_indent(l, indent + 2)) {
            if item_line(lines[*i], indent + 2).is_some() {
                if !pending.is_empty() {
                    content.push(paragraph(&pending));
                    pending.clear();
                }
                content.push(parse_list(lines, i, indent + 2));
            } else {
                pending.push(deeper);
                *i += 1;
            }
        }
        if !pending.is_empty() {
            content.push(paragraph(&pending));
        }
        items.push(if kind == "taskList" {
            json!({ "type": "taskItem", "attrs": { "checked": item.checked }, "content": content })
        } else {
            json!({ "type": "listItem", "content": content })
        });
    }
    let mut list = json!({ "type": kind, "content": items });
    if kind == "orderedList" && start != 1 {
        list["attrs"] = json!({ "start": start });
    }
    list
}

fn parse_blocks(markdown: &str) -> Vec<Value> {
    let normalized = markdown.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if RULE.is_match(line) {
            blocks.push(json!({ "type": "horizontalRule" }));
            i += 1;
            continue;
        }
        if let Some(m) = IMAGE.captures(line) {
            blocks.push(image_node(&m));
            i += 1;
            continue;
        }
        // What a placeholder stood for can't be rebuilt from markdown.
        if PLACEHOLDER.is_match(line) {
            i += 1;
            continue;
        }
        if let Some(m) = HEADING.captures(line) {
            blocks.push(json!({
                "type": "heading", "attrs": { "level": m[1].len() }, "content": parse_inline(&m[2], &[]),
            }));
            i += 1;
            continue;
        }
        if let Some(info) = line.strip_prefix("```") {
            let language = Some(info.trim()).filter(|l| !l.is_empty());
            i += 1;
            let mut code = Vec::new();
            while i < lines.len() && !lines[i].starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1; // closing fence
            let content = if code.is_empty() {
                vec![]
            } else {
                vec![mk_text(&code.join("\n"), &[])]
            };
            blocks.push(json!({ "type": "codeBlock", "attrs": { "language": language }, "content": content }));
            continue;
        }
        if line.starts_with('>') {
            let mut quoted = Vec::new();
            while let Some(rest) = lines.get(i).and_then(|l| l.strip_prefix('>')) {
                quoted.push(rest.strip_prefix(' ').unwrap_or(rest));
                i += 1;
            }
            // A bare `>` line separates the quote's paragraphs.
            let mut paragraphs: Vec<Value> = quoted
                .split(|l| l.trim().is_empty())
                .filter(|p| !p.is_empty())
                .map(paragraph)
                .collect();
            if paragraphs.is_empty() {
                paragraphs.push(paragraph(&[]));
            }
            blocks.push(json!({ "type": "blockquote", "content": paragraphs }));
            continue;
        }
        if item_line(line, 0).is_some() {
            blocks.push(parse_list(&lines, &mut i, 0));
            continue;
        }
        let mut para = vec![line];
        i += 1;
        while i < lines.len() && !lines[i].trim().is_empty() && !starts_block(lines[i]) {
            para.push(lines[i]);
            i += 1;
        }
        blocks.push(paragraph(&para));
    }
    blocks
}

fn walk(node: &Value, f: &mut impl FnMut(&Value)) {
    f(node);
    for child in node["content"].as_array().into_iter().flatten() {
        walk(child, f);
    }
}

fn walk_mut(node: &mut Value, f: &mut impl FnMut(&mut Value)) {
    f(node);
    if let Some(children) = node.get_mut("content").and_then(Value::as_array_mut) {
        for child in children {
            walk_mut(child, f);
        }
    }
}

/// Replace each chip that has no id with its label as plain text, then merge the
/// adjacent unmarked text runs that leaves.
fn drop_unresolved(node: &mut Value) {
    let Some(content) = node.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    let is_plain_text = |n: &Value| n["type"] == "text" && n.get("marks").is_none();
    let mut out: Vec<Value> = Vec::new();
    for child in content.drain(..) {
        let is_chip = child["type"] == "mention" || child["type"] == "hashtag";
        let unresolved = is_chip && child["attrs"]["id"].as_str().is_none_or(str::is_empty);
        let mut next = if unresolved {
            mk_text(child["attrs"]["label"].as_str().unwrap_or(""), &[])
        } else {
            child
        };
        if is_plain_text(&next) {
            if let Some(prev) = out.last_mut().filter(|p| is_plain_text(p)) {
                let joined = format!(
                    "{}{}",
                    prev["text"].as_str().unwrap_or(""),
                    next["text"].as_str().unwrap_or("")
                );
                prev["text"] = Value::String(joined);
                continue;
            }
            if next["text"].as_str().is_none_or(str::is_empty) {
                continue;
            }
        }
        drop_unresolved(&mut next);
        out.push(next);
    }
    *content = out;
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
        // An unaliased [[Todo/Ship]] is labelled "Ship", not the prefixed text.
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

// ---------------------------------------------------------------- doc -> markdown

fn is_chip(n: &Value) -> bool {
    n["type"] == "mention" || n["type"] == "hashtag"
}

/// The ids of every mention/hashtag in the doc, de-duplicated.
pub fn chip_ids(doc: &Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    walk(doc, &mut |n| {
        if let Some(id) = n["attrs"]["id"]
            .as_str()
            .filter(|id| is_chip(n) && !id.is_empty())
        {
            if !ids.iter().any(|seen| seen == id) {
                ids.push(id.to_string());
            }
        }
    });
    ids
}

/// Plain text written so it reads back as itself: `\`, `*`, `` ` `` and `[`
/// are always escaped, as is a `]` before `(` (which would end a link's text)
/// and a `~` before `~` or at the end (which could open or close `~~`). So is
/// a `#` or an ISO date at a word boundary, where it would become a chip.
/// `prev` is the character written just before; `None` at the start of a run.
fn escape_text(text: &str, mut prev: Option<char>) -> String {
    let mut out = String::new();
    for (at, c) in text.char_indices() {
        let rest = &text[at..];
        let at_boundary = prev.is_none_or(char::is_whitespace);
        let escape = match c {
            '\\' | '*' | '`' | '[' => true,
            ']' => rest[1..].starts_with('('),
            '~' => rest[1..].is_empty() || rest[1..].starts_with('~'),
            '#' => at_boundary,
            _ => at_boundary && date_at(rest).is_some(),
        };
        if escape {
            out.push('\\');
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

/// The marks of `node` that markdown writes. Code is verbatim, so it is written
/// alone; `inexpressible` reports what that drops.
fn writable_marks(node: &Value) -> Vec<Mark> {
    let marks: Vec<Mark> = node["marks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Mark::from_json)
        .collect();
    if marks.contains(&Mark::Code) {
        vec![Mark::Code]
    } else {
        marks
    }
}

/// `inner` inside the delimiters of each emphasis in `marks`.
fn wrap_emphasis(mut inner: String, marks: &[Mark]) -> String {
    for (mark, delim) in [
        (Mark::Strike, "~~"),
        (Mark::Bold, "**"),
        (Mark::Italic, "*"),
    ] {
        if marks.contains(&mark) {
            inner = format!("{delim}{inner}{delim}");
        }
    }
    inner
}

/// Text with every mark in `marks` but a link.
fn text_markdown(text: &str, marks: &[Mark], prev: Option<char>) -> String {
    if marks.contains(&Mark::Code) {
        return format!("`{text}`");
    }
    // Emphasis is read as a run of its own, which starts at a word boundary.
    wrap_emphasis(
        escape_text(text, if marks.is_empty() { prev } else { None }),
        marks,
    )
}

/// The href of a text node's writable link.
fn link_of(node: &Value) -> Option<String> {
    if node["type"] != "text" {
        return None;
    }
    writable_marks(node).into_iter().find_map(|m| match m {
        Mark::Link(href) => Some(href),
        _ => None,
    })
}

/// Text nodes that share one link, as one `[...](href)`. Emphasis on all of
/// them is written outside the brackets (`**[a](u)**`), the rest inside.
fn link_markdown(run: &[Value], href: &str) -> String {
    let marks: Vec<Vec<Mark>> = run
        .iter()
        .map(|n| {
            writable_marks(n)
                .into_iter()
                .filter(|m| !matches!(m, Mark::Link(_)))
                .collect()
        })
        .collect();
    let shared: Vec<Mark> = marks[0]
        .iter()
        .filter(|m| marks.iter().all(|ms| ms.contains(m)))
        .cloned()
        .collect();
    let mut text = String::new();
    for (node, node_marks) in run.iter().zip(&marks) {
        let own: Vec<Mark> = node_marks
            .iter()
            .filter(|m| !shared.contains(m))
            .cloned()
            .collect();
        text += &text_markdown(
            node["text"].as_str().unwrap_or(""),
            &own,
            text.chars().next_back(),
        );
    }
    // An href reads `\X` as X; nothing else in it is syntax.
    wrap_emphasis(format!("[{text}]({})", href.replace('\\', "\\\\")), &shared)
}

/// [[Name]] for a Note, unless its name itself reads as [[Structure/Name]].
pub fn mention_markdown(target: &ChipTarget) -> String {
    let bare = target.structure_type == DEFAULT_MENTION_STRUCTURE
        && split_structure_prefix(&target.name).structure_type == DEFAULT_MENTION_STRUCTURE;
    if bare {
        format!("[[{}]]", target.name)
    } else {
        format!("[[{}/{}]]", target.structure_type, target.name)
    }
}

fn chip_markdown(n: &Value, targets: &HashMap<String, ChipTarget>, prev: Option<char>) -> String {
    let id = n["attrs"]["id"].as_str().unwrap_or("");
    let Some(target) = targets.get(id) else {
        return escape_text(n["attrs"]["label"].as_str().unwrap_or(""), prev);
    };
    if n["type"] == "hashtag" {
        format!("#{}", target.name)
    } else {
        mention_markdown(target)
    }
}

/// Inline nodes as markdown; a hard break ends its line with `\`. A trailing
/// hard break has no markdown form (a paragraph can't end in one) and is left out.
fn inline_markdown(content: &Value, targets: &HashMap<String, ChipTarget>) -> String {
    let Some(nodes) = content.as_array() else {
        return String::new();
    };
    let nodes = &nodes[..nodes.len()
        - nodes
            .iter()
            .rev()
            .take_while(|n| n["type"] == "hardBreak")
            .count()];
    let mut out = String::new();
    let mut i = 0;
    while i < nodes.len() {
        let n = &nodes[i];
        if let Some(href) = link_of(n) {
            let run = nodes[i..]
                .iter()
                .take_while(|m| link_of(m).as_ref() == Some(&href))
                .count();
            out += &link_markdown(&nodes[i..i + run], &href);
            i += run;
            continue;
        }
        let prev = out.chars().next_back();
        let md = match n["type"].as_str() {
            Some("text") => {
                text_markdown(n["text"].as_str().unwrap_or(""), &writable_marks(n), prev)
            }
            Some("mention" | "hashtag") => chip_markdown(n, targets, prev),
            Some("dateChip") => n["attrs"]["date"].as_str().unwrap_or("").to_string(),
            Some("hardBreak") => "\\\n".to_string(),
            _ => inline_markdown(&n["content"], targets),
        };
        out += &md;
        i += 1;
    }
    out
}

/// A paragraph's lines, each escaped with a leading `\` where it would
/// otherwise open a block (`- `, `1. `, `>`, `---`, an image line).
fn paragraph_markdown(p: &Value, targets: &HashMap<String, ChipTarget>) -> String {
    inline_markdown(&p["content"], targets)
        .split('\n')
        .map(|line| {
            if starts_block(line) {
                format!("\\{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A child of a list item or quote: paragraphs are escaped like top-level ones.
fn child_markdown(child: &Value, targets: &HashMap<String, ChipTarget>) -> String {
    if child["type"] == "paragraph" {
        paragraph_markdown(child, targets)
    } else {
        block_markdown(child, targets)
    }
}

/// One item: `marker` before its first line, two spaces before the rest (its
/// continuation lines and nested lists).
fn item_markdown(item: &Value, marker: &str, targets: &HashMap<String, ChipTarget>) -> String {
    let mut lines: Vec<String> = Vec::new();
    // Item text can't read as a task's `[ ]`: escape_text escapes every `[`.
    for child in item["content"].as_array().into_iter().flatten() {
        lines.extend(child_markdown(child, targets).split('\n').map(String::from));
    }
    let mut out = format!("{marker}{}", lines.first().map_or("", String::as_str));
    for line in lines.iter().skip(1) {
        out.push('\n');
        if !line.is_empty() {
            out += "  ";
            out += line;
        }
    }
    out
}

/// One marker line per item: `- `, `N. ` from the list's start, or a task's
/// `- [ ] ` / `- [x] `.
fn list_markdown(list: &Value, targets: &HashMap<String, ChipTarget>) -> String {
    let start = list["attrs"]["start"].as_u64().unwrap_or(1);
    let items = list["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let marker = match list["type"].as_str() {
                Some("orderedList") => format!("{}. ", start + idx as u64),
                Some("taskList") if item["attrs"]["checked"] == true => "- [x] ".to_string(),
                Some("taskList") => "- [ ] ".to_string(),
                _ => "- ".to_string(),
            };
            item_markdown(item, &marker, targets)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each paragraph's lines after `> `, the paragraphs separated by a bare `>`.
fn blockquote_markdown(quote: &Value, targets: &HashMap<String, ChipTarget>) -> String {
    let children = quote["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    children
        .iter()
        .map(|child| {
            let md = child_markdown(child, targets);
            md.split('\n')
                .map(|l| {
                    if l.is_empty() {
                        ">".to_string()
                    } else {
                        format!("> {l}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n>\n")
}

/// The line standing in for a node markdown can't express.
fn placeholder(node_type: &str) -> String {
    format!("<!-- calcifer:{node_type} -->")
}

/// `![alt](src "title")`, or `None` when the src can't sit in that syntax or
/// is inline data (a pasted image), which is no use to read.
fn image_markdown(attrs: &Value) -> Option<String> {
    let src = attrs["src"]
        .as_str()
        .filter(|s| !s.is_empty() && !s.starts_with("data:"))?;
    if src.contains(|c: char| c.is_whitespace() || "()<>\"".contains(c)) {
        return None;
    }
    let escape = |text: &str, special: char| {
        text.chars().fold(String::new(), |mut out, c| {
            if c == '\\' || c == special {
                out.push('\\');
            }
            out.push(c);
            out
        })
    };
    let alt = escape(attrs["alt"].as_str().unwrap_or(""), ']');
    let title = attrs["title"].as_str().filter(|t| !t.is_empty());
    let title = title
        .map(|t| format!(" \"{}\"", escape(t, '"')))
        .unwrap_or_default();
    Some(format!("![{alt}]({src}{title})"))
}

/// An image whose markdown carries all of it: the editor's caption, width and
/// alignment have no markdown form.
fn image_is_expressible(attrs: &Value) -> bool {
    image_markdown(attrs).is_some()
        && attrs["caption"].as_str().is_none_or(str::is_empty)
        && attrs["width"].is_null()
        && attrs["alignment"].as_str().is_none_or(|a| a == "left")
}

fn is_list(n: &Value) -> bool {
    matches!(
        n["type"].as_str(),
        Some("bulletList" | "orderedList" | "taskList")
    )
}

/// `noun` after "a" or "an".
fn with_article(noun: &str) -> String {
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
fn attr_fits(node_type: &str, key: &str, value: &Value) -> bool {
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
        // A chip is written from its target, whatever its stored label says.
        (
            "mention" | "hashtag",
            "id" | "label" | "structureType" | "char" | "mentionSuggestionChar",
        ) => true,
        // Checked by image_is_expressible.
        ("image", "src" | "alt" | "title" | "caption" | "width" | "alignment") => true,
        _ => value.is_null(),
    }
}

/// What markdown would drop of `node`'s own marks: a mark it doesn't write,
/// a link it can't, or code it can't (code is verbatim and stands alone).
fn inexpressible_marks(node: &Value) -> Option<String> {
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
const KNOWN_NODES: [&str; 16] = [
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

/// One top-level block as markdown, without a trailing newline. Empty for a
/// block with nothing to show. `targets` maps each chip id (see `chip_ids`) to
/// its target's current identity; a missing id means the target is gone, and
/// the chip is written as its stored label in plain text, so writing it back
/// creates nothing.
pub fn block_markdown(block: &Value, targets: &HashMap<String, ChipTarget>) -> String {
    match block["type"].as_str() {
        Some("heading") => {
            let level = block["attrs"]["level"]
                .as_u64()
                .filter(|&l| l > 0)
                .unwrap_or(1) as usize;
            format!(
                "{} {}",
                "#".repeat(level),
                inline_markdown(&block["content"], targets)
            )
        }
        Some("bulletList" | "orderedList" | "taskList") => list_markdown(block, targets),
        Some("blockquote") => blockquote_markdown(block, targets),
        Some("codeBlock") => format!(
            "```{}\n{}\n```",
            block["attrs"]["language"].as_str().unwrap_or(""),
            block["content"][0]["text"].as_str().unwrap_or(""),
        ),
        Some("horizontalRule") => "---".to_string(),
        Some("image") => image_markdown(&block["attrs"]).unwrap_or_else(|| placeholder("image")),
        Some("paragraph") => paragraph_markdown(block, targets),
        other => placeholder(other.unwrap_or("node")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake server: ids are "Structure:Name". A DailyNote isn't creatable, so
    /// only "June 13, 2026" exists; any other DailyNote name is a miss.
    fn resolve(parsed: Parsed) -> Value {
        let ids = parsed
            .references
            .iter()
            .filter(|r| r.structure_type != "DailyNote" || r.name == "June 13, 2026")
            .map(|r| (r.key(), format!("{}:{}", r.structure_type, r.name)))
            .collect();
        parsed.resolve(&ids)
    }

    fn to_tt(md: &str) -> Value {
        resolve(parse(md))
    }

    /// Every chip's target, read back from its fake id (all targets still exist).
    fn fake_targets(doc: &Value) -> HashMap<String, ChipTarget> {
        chip_ids(doc)
            .into_iter()
            .map(|id| {
                let (structure_type, name) = id.split_once(':').unwrap();
                let target = ChipTarget {
                    structure_type: structure_type.into(),
                    name: name.into(),
                };
                (id, target)
            })
            .collect()
    }

    fn to_md_with(doc: &Value, targets: &HashMap<String, ChipTarget>) -> String {
        let blocks: Vec<String> = doc["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| block_markdown(b, targets))
            .filter(|s| !s.is_empty())
            .collect();
        blocks.join("\n\n")
    }

    fn to_md(doc: &Value) -> String {
        to_md_with(doc, &fake_targets(doc))
    }

    fn collect(doc: &Value, pick: impl Fn(&Value) -> Option<String>) -> Vec<String> {
        let mut out = Vec::new();
        walk(doc, &mut |n| out.extend(pick(n)));
        out
    }

    fn node_types(doc: &Value) -> Vec<String> {
        collect(doc, |n| n["type"].as_str().map(String::from))
    }

    fn chips(doc: &Value) -> Vec<Value> {
        let mut out = Vec::new();
        walk(doc, &mut |n| {
            if is_chip(n) {
                out.push(n["attrs"].clone());
            }
        });
        out
    }

    fn dates(md: &str) -> Vec<String> {
        collect(&to_tt(md), |n| {
            (n["type"] == "dateChip").then(|| n["attrs"]["date"].as_str().unwrap().into())
        })
    }

    #[test]
    fn markdown_round_trips() {
        let cases = [
            "See [[Photosynthesis]] and #biology on 2026-06-15.",
            "# Heading\n\nA paragraph with [[Krebs Cycle]] and #metabolism.",
            "- one\n- two with [[Link]]\n- three",
            "1. first\n2. second",
            "> quoted [[Link]]",
            "```rust\nlet x = 1;\n```",
            "Search by *meaning*, not keywords, via **mean pooling** and `cosine similarity`.",
            "Ship [[Todo/Ship It]] by [[DailyNote/June 13, 2026]]; see [[Tag/ml]].",
            "A note named [[Note/Todo/Ship]] and one named [[Foo/Bar]].",
            "Above.\n\n---\n\nBelow.",
            "![a diagram](https://example.com/d.png)\n\n![](/local.png \"The \\\"title\\\"\")\n\n![a \\] bracket](x.png)",
            // Plain text that would read as syntax.
            "Plain \\*x\\*, \\#word, \\[\\[x]], \\2026-06-13, \\`tick\\` and C:\\\\dir.",
            "\\- not a list\\\n\\1. nor this\\\n\\> nor a quote\n\n- \\[ ] not a task",
            "***both*** and **bold** then *it*",
            // Nested lists, two spaces a level.
            "- one\n  - nested [[Link]]\n    - deeper\n  - nested two\n- two",
            "1. first\n  1. inner\n  2. inner two\n2. second\n  - mixed",
            "3. starts at three\n4. four",
            // Task lists.
            "- [ ] open\n- [x] done [[Link]]\n  - [ ] nested task",
            // Hard breaks.
            "First line\\\nsecond line\\\n\\\nafter a blank one",
            "- item\\\n  continued\n  - nested",
            // Quotes with several paragraphs.
            "> one\n>\n> two [[Link]]\\\n> broken",
            // Links and strike.
            "[a link](https://example.com)",
            "see [**bold** text](u) here",
            "**[a](u)**",
            "*[a **b**](https://x.org/w/F_(y)?q=*~\\\\)* and ~~[e](u)~~",
            "~~gone~~",
            "Plain \\[a], b] \\[x\\](x) \\~~ and a\\~",
        ];
        for md in cases {
            assert_eq!(to_md(&to_tt(md)), md, "round-trip of {md:?}");
        }

        // A doc whose plain text reads as syntax comes back byte-identical.
        let text = "Plain *x*, #word, [[x]] and 2026-06-13, a \\ and `tick`.";
        let doc = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }] });
        assert_eq!(to_tt(&to_md(&doc)), doc, "{}", to_md(&doc));
        let lines = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
            { "type": "text", "text": "- a" }, { "type": "hardBreak" }, { "type": "text", "text": "1. b" },
            { "type": "hardBreak" }, { "type": "text", "text": "---" },
        ] }] });
        assert_eq!(to_tt(&to_md(&lines)), lines, "{}", to_md(&lines));
    }

    fn link(href: &str) -> Value {
        Mark::Link(href.into()).to_json()
    }

    fn text(text: &str, marks: Value) -> Value {
        json!({ "type": "text", "text": text, "marks": marks })
    }

    fn paragraph_doc(content: Value) -> Value {
        json!({ "type": "doc", "content": [{ "type": "paragraph", "content": content }] })
    }

    #[test]
    fn links_and_strike_round_trip_from_the_doc() {
        // One link over three text nodes, the marks in the editor's order.
        let doc = paragraph_doc(json!([
            { "type": "text", "text": "see " },
            text("a ", json!([link("https://example.com")])),
            text("bold", json!([link("https://example.com"), { "type": "bold" }, { "type": "strike" }])),
            text(" word", json!([link("https://example.com"), { "type": "italic" }])),
            { "type": "text", "text": "." },
        ]));
        assert_eq!(
            to_md(&doc),
            "see [a **~~bold~~*** word*](https://example.com)."
        );
        assert_eq!(to_tt(&to_md(&doc)), doc);

        let plain = paragraph_doc(json!([{ "type": "text", "text": "see [x](y) and ~~z~~" }]));
        assert_eq!(to_md(&plain), "see \\[x\\](y) and \\~~z\\~\\~");
        assert_eq!(to_tt(&to_md(&plain)), plain);
    }

    #[test]
    fn links_parse_to_the_editors_link_mark() {
        let marks = |md: &str| collect(&to_tt(md), |n| n.get("marks").map(Value::to_string));
        assert_eq!(marks("[a](u)"), [json!([link("u")]).to_string()]);
        assert_eq!(
            marks("**[a](u)**"),
            [json!([link("u"), { "type": "bold" }]).to_string()]
        );
        // The editor's code mark excludes every other.
        assert_eq!(
            marks("**[`a`](u)**"),
            [json!([{ "type": "code" }]).to_string()]
        );
        // Not links: an empty text or href, a space in the href, an unbalanced paren.
        for md in ["[](u)", "[a]()", "[a](b c)", "[a](b(c)", "[a] (u)"] {
            assert_eq!(marks(md), Vec::<String>::new(), "{md}");
        }
        // The first `](` ends a link's text, which holds no link of its own.
        assert_eq!(to_md(&to_tt("[a [b](v)](u)")), "[a \\[b](v)\\](u)");
    }

    #[test]
    fn nested_and_task_lists_parse_to_editor_nodes() {
        let doc = to_tt("- [x] done\n  - [ ] sub\n- [ ] open");
        assert_eq!(doc["content"][0]["type"], "taskList");
        let first = &doc["content"][0]["content"][0];
        assert_eq!(
            (&first["type"], &first["attrs"]["checked"]),
            (&json!("taskItem"), &json!(true))
        );
        assert_eq!(first["content"][1]["content"][0]["attrs"]["checked"], false);
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 2);

        let ordered = to_tt("1. a\n  - b");
        assert_eq!(
            node_types(&ordered),
            [
                "doc",
                "orderedList",
                "listItem",
                "paragraph",
                "text",
                "bulletList",
                "listItem",
                "paragraph",
                "text"
            ]
        );
        let broken = to_tt("a\\\nb");
        assert_eq!(
            node_types(&broken),
            ["doc", "paragraph", "text", "hardBreak", "text"]
        );
        assert_eq!(
            to_tt("> a\n>\n> b")["content"][0]["content"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn emits_chip_nodes_and_marks() {
        let types = node_types(&to_tt("See [[Photosynthesis]] and #biology on 2026-06-15."));
        for want in ["mention", "hashtag", "dateChip"] {
            assert!(types.iter().any(|t| t == want), "{want} in {types:?}");
        }
        let marked = to_tt("*a* **b** `c`");
        let marks = collect(&marked, |n| {
            n["marks"].as_array().map(|ms| {
                ms.iter()
                    .map(|m| m["type"].as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(",")
            })
        });
        assert_eq!(marks, ["italic", "bold", "code"]);
    }

    #[test]
    fn underscores_are_not_emphasis() {
        assert_eq!(
            to_md(&to_tt("the one_daily_note_per_day index")),
            "the one_daily_note_per_day index"
        );
    }

    #[test]
    fn only_real_calendar_days_become_date_chips() {
        for good in ["2026-02-28", "2028-02-29"] {
            assert_eq!(dates(&format!("Due {good}.")), [good]);
        }
        // Punctuation or a space after a date is fine; more digits on either side isn't.
        assert_eq!(dates("Due 2026-06-15, then"), ["2026-06-15"]);
        assert_eq!(dates("Due 2026-06-15 then"), ["2026-06-15"]);
        for bad in [
            "2026-02-30",
            "2026-13-45",
            "2027-02-29",
            "2026-04-31",
            "2026-00-10",
            "2026-01-00",
            "2026-06-155",
            "2026-06-15123",
            "12026-06-15",
        ] {
            let md = format!("Due {bad}.");
            assert!(dates(&md).is_empty(), "{bad} is not a date chip");
            assert_eq!(to_md(&to_tt(&md)), md, "{bad} stays plain text");
        }
    }

    #[test]
    fn structure_prefix_targets_only_registry_types() {
        let todo = &chips(&to_tt("[[Todo/Ship]]"))[0];
        assert_eq!(
            (&todo["structureType"], &todo["id"], &todo["label"]),
            (&json!("Todo"), &json!("Todo:Ship"), &json!("Ship"))
        );
        let foo = &chips(&to_tt("[[Foo/Bar]]"))[0];
        assert_eq!(
            (&foo["structureType"], &foo["id"]),
            (&json!("Note"), &json!("Note:Foo/Bar"))
        );
        let lower = &chips(&to_tt("[[todo/Ship]]"))[0];
        assert_eq!(
            lower["id"], "Note:todo/Ship",
            "the prefix must match a structure type exactly"
        );
    }

    #[test]
    fn alias_is_accepted_but_written_from_the_target_name() {
        let aliased = to_tt("See [[Attention|attn]].");
        assert_eq!(chips(&aliased)[0]["id"], "Note:Attention");
        assert_eq!(to_md(&aliased), "See [[Attention]].");
    }

    #[test]
    fn a_missing_non_creatable_target_is_plain_text() {
        let miss = to_tt("On [[DailyNote/June 14, 2026]] we shipped.");
        assert!(chips(&miss).is_empty());
        assert_eq!(to_md(&miss), "On June 14, 2026 we shipped.");
    }

    #[test]
    fn chips_are_written_from_current_names() {
        let doc = to_tt("See [[Old Name]], [[Gone]] and #oldtag.");
        let targets = HashMap::from([
            (
                "Note:Old Name".to_string(),
                ChipTarget {
                    structure_type: "Note".into(),
                    name: "New Name".into(),
                },
            ),
            (
                "Tag:oldtag".to_string(),
                ChipTarget {
                    structure_type: "Tag".into(),
                    name: "newtag".into(),
                },
            ),
        ]);
        assert_eq!(
            to_md_with(&doc, &targets),
            "See [[New Name]], Gone and #newtag."
        );
    }

    #[test]
    fn references_are_deduplicated_case_insensitively() {
        let parsed = parse("[[Attention]] and [[attention]] and [[Todo/Ship]]");
        let keys: Vec<_> = parsed
            .references
            .iter()
            .map(|r| (r.structure_type.as_str(), r.name.as_str()))
            .collect();
        assert_eq!(keys, [("Note", "Attention"), ("Todo", "Ship")]);
    }

    #[test]
    fn empty_markdown_is_one_empty_paragraph() {
        assert_eq!(
            to_tt("  \n"),
            json!({ "type": "doc", "content": [{ "type": "paragraph" }] })
        );
    }

    #[test]
    fn unknown_nodes_are_placeholders_and_unknown_marks_are_left_alone() {
        let doc = json!({ "type": "doc", "content": [
            { "type": "paragraph", "content": [{ "type": "text", "text": "x", "marks": [{ "type": "underline" }] }] },
            { "type": "table", "content": [] },
            { "type": "image", "attrs": { "src": "data:image/png;base64,AAAA" } },
        ]});
        assert_eq!(
            to_md(&doc),
            "x\n\n<!-- calcifer:table -->\n\n<!-- calcifer:image -->"
        );
        // Reading a placeholder back writes nothing in its place.
        assert_eq!(
            node_types(&to_tt(&to_md(&doc))),
            ["doc", "paragraph", "text"]
        );
    }

    #[test]
    fn inexpressible_names_what_markdown_would_lose() {
        let image = |attrs: Value| json!({ "type": "image", "attrs": attrs });
        assert_eq!(
            inexpressible(&image(
                json!({ "src": "a.png", "alt": "a", "alignment": "left" })
            )),
            None
        );
        assert_eq!(
            inexpressible(&image(json!({ "src": "a.png", "width": "50%" }))).as_deref(),
            Some("an image")
        );
        assert_eq!(
            inexpressible(&image(json!({ "src": "data:image/png;base64,AAAA" }))).as_deref(),
            Some("an image")
        );
        assert_eq!(
            inexpressible(&image(json!({ "src": "a.png", "height": 40 }))).as_deref(),
            Some("an image's height")
        );
        assert_eq!(
            inexpressible(&json!({ "type": "table" })).as_deref(),
            Some("a table")
        );
        assert_eq!(
            inexpressible(&to_tt("Plain [[Link]] #tag 2026-06-13.")["content"][0]),
            None
        );
        assert_eq!(
            inexpressible(&to_tt("- a\n  - [x] b\n- c\\\n  d")["content"][0]),
            None
        );
        let quoted_list =
            json!({ "type": "blockquote", "content": [{ "type": "bulletList", "content": [] }] });
        assert_eq!(inexpressible(&quoted_list).as_deref(), Some("a blockquote"));
        let para = json!({ "type": "paragraph", "content": [] });
        let two_paragraphs = json!({ "type": "bulletList", "content": [{ "type": "listItem", "content": [para, para] }] });
        assert_eq!(
            inexpressible(&two_paragraphs).as_deref(),
            Some("a listItem")
        );
    }

    #[test]
    fn inexpressible_names_formatting_markdown_would_lose() {
        let marked = |marks: Value| paragraph_doc(json!([text("x", marks)]))["content"][0].clone();
        let mut odd_link = link("u");
        odd_link["attrs"]["target"] = json!("_self");
        let cases = [
            (
                marked(json!([{ "type": "underline" }])),
                "underline formatting",
            ),
            (
                marked(json!([{ "type": "highlight", "attrs": { "color": "#ff0" } }])),
                "highlight formatting",
            ),
            (
                marked(
                    json!([{ "type": "textStyle", "attrs": { "color": "red", "fontFamily": null } }]),
                ),
                "textStyle formatting",
            ),
            (
                marked(json!([{ "type": "subscript" }])),
                "subscript formatting",
            ),
            (
                marked(json!([{ "type": "bold" }, { "type": "code" }])),
                "code combined with other formatting",
            ),
            (marked(json!([odd_link])), "a link"),
            (marked(json!([link("a b")])), "a link"),
            (
                json!({ "type": "paragraph", "content": [text("a`b", json!([{ "type": "code" }]))] }),
                "inline code containing a backtick",
            ),
            (
                json!({ "type": "paragraph", "content": [{
                    "type": "mention", "attrs": { "id": "Note:A", "label": "A" }, "marks": [{ "type": "bold" }],
                }] }),
                "formatting on a mention",
            ),
            (
                json!({ "type": "heading", "attrs": { "level": 2, "textAlign": "center" } }),
                "a heading's textAlign",
            ),
            (
                json!({ "type": "orderedList", "attrs": { "start": 1, "type": "a" } }),
                "an orderedList's type",
            ),
        ];
        for (block, want) in cases {
            assert_eq!(inexpressible(&block).as_deref(), Some(want), "{block}");
        }

        let editor_link = json!({ "type": "link", "attrs": {
            "href": "https://example.com", "target": "_blank", "rel": "noopener noreferrer", "class": null, "title": null,
        } });
        for block in [
            marked(json!([editor_link])),
            marked(json!([{ "type": "link", "attrs": { "href": "u" } }, { "type": "bold" }])),
            marked(json!([{ "type": "strike" }])),
            json!({ "type": "heading", "attrs": { "level": 2, "textAlign": null }, "content": [] }),
            json!({ "type": "paragraph", "attrs": { "textAlign": null }, "content": [{ "type": "dateChip", "attrs": { "date": "2026-09-30" } }] }),
        ] {
            assert_eq!(inexpressible(&block), None, "{block}");
        }
    }
}
