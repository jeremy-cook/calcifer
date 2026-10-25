use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use super::inline::{mk_text, parse_inline, unescape};

pub(super) static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.*)$").unwrap());

/// A list item's marker: `- ` or `* ` (a task item when `[ ]` or `[x]`
/// follows), or `N. `.
pub(super) static LIST_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[-*]\s+(?:\[([ xX])\](?:\s+|$))?|([0-9]+)\.\s+)").unwrap());

pub(super) static SPECIAL_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6}\s|```|>|[-*]\s+|[0-9]+\.\s+)").unwrap());

pub(super) static RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^---\s*$").unwrap());

/// `![alt](src "title")`: `\` escapes in alt and title, no space or quote in src.
pub(super) static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^!\[((?:[^\]\\]|\\.)*)\]\(([^\s()<>"]+)(?:\s+"((?:[^"\\]|\\.)*)")?\)\s*$"#)
        .unwrap()
});

pub(super) static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<!-- calcifer:\S+ -->\s*$").unwrap());

/// A line that opens a block of its own, so it never continues a paragraph.
pub(super) fn starts_block(line: &str) -> bool {
    SPECIAL_BLOCK.is_match(line)
        || RULE.is_match(line)
        || IMAGE.is_match(line)
        || PLACEHOLDER.is_match(line)
}

pub(super) fn image_node(m: &regex::Captures) -> Value {
    let attr = |i: usize| {
        m.get(i)
            .map(|t| unescape(t.as_str()))
            .filter(|t| !t.is_empty())
    };
    json!({ "type": "image", "attrs": { "src": &m[2], "alt": attr(1), "title": attr(3) } })
}

/// A line ending in a `\` that isn't itself escaped: a hard break.
pub(super) fn ends_in_break(line: &str) -> bool {
    line.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1
}

/// A block's source lines as one inline run: a line ending in a hard break
/// joins the next with `\n`, any other line break is a space. A `\` ending the
/// last line breaks nothing and stays literal.
pub(super) fn join_lines(lines: &[&str]) -> String {
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

pub(super) fn paragraph(lines: &[&str]) -> Value {
    json!({ "type": "paragraph", "content": parse_inline(&join_lines(lines), &[]) })
}

/// `line` after exactly `indent` spaces, when something other than space follows.
pub(super) fn at_indent(line: &str, indent: usize) -> Option<&str> {
    let rest = line.get(indent..)?;
    let indented = line.as_bytes()[..indent].iter().all(|&b| b == b' ');
    (indented && !rest.is_empty() && !rest.starts_with(char::is_whitespace)).then_some(rest)
}

/// A list item's first line.
pub(super) struct ItemLine<'a> {
    kind: &'static str,
    text: &'a str,
    checked: bool,
    number: u64,
}

/// The item `line` opens at exactly `indent` spaces, if any.
pub(super) fn item_line(line: &str, indent: usize) -> Option<ItemLine<'_>> {
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
pub(super) fn parse_list(lines: &[&str], i: &mut usize, indent: usize) -> Value {
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

pub(super) fn parse_blocks(markdown: &str) -> Vec<Value> {
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
