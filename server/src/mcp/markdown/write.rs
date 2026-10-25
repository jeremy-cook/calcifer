use std::collections::HashMap;

use serde_json::Value;

use super::blocks::starts_block;
use super::inline::{date_at, Mark};
use super::{split_structure_prefix, ChipTarget, DEFAULT_MENTION_STRUCTURE};

/// Plain text written so it reads back as itself: `\`, `*`, `` ` `` and `[`
/// are always escaped, as is a `]` before `(` (which would end a link's text)
/// and a `~` before `~` or at the end (which could open or close `~~`). So is
/// a `#` or an ISO date at a word boundary, where it would become a chip.
/// `prev` is the character written just before; `None` at the start of a run.
pub(super) fn escape_text(text: &str, mut prev: Option<char>) -> String {
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
pub(super) fn writable_marks(node: &Value) -> Vec<Mark> {
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
pub(super) fn wrap_emphasis(mut inner: String, marks: &[Mark]) -> String {
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
pub(super) fn text_markdown(text: &str, marks: &[Mark], prev: Option<char>) -> String {
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
pub(super) fn link_of(node: &Value) -> Option<String> {
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
pub(super) fn link_markdown(run: &[Value], href: &str) -> String {
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

pub(super) fn chip_markdown(
    n: &Value,
    targets: &HashMap<String, ChipTarget>,
    prev: Option<char>,
) -> String {
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
pub(super) fn inline_markdown(content: &Value, targets: &HashMap<String, ChipTarget>) -> String {
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
pub(super) fn paragraph_markdown(p: &Value, targets: &HashMap<String, ChipTarget>) -> String {
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
pub(super) fn child_markdown(child: &Value, targets: &HashMap<String, ChipTarget>) -> String {
    if child["type"] == "paragraph" {
        paragraph_markdown(child, targets)
    } else {
        block_markdown(child, targets)
    }
}

/// One item: `marker` before its first line, two spaces before the rest (its
/// continuation lines and nested lists).
pub(super) fn item_markdown(
    item: &Value,
    marker: &str,
    targets: &HashMap<String, ChipTarget>,
) -> String {
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
pub(super) fn list_markdown(list: &Value, targets: &HashMap<String, ChipTarget>) -> String {
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
pub(super) fn blockquote_markdown(quote: &Value, targets: &HashMap<String, ChipTarget>) -> String {
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
pub(super) fn placeholder(node_type: &str) -> String {
    format!("<!-- calcifer:{node_type} -->")
}

/// `![alt](src "title")`, or `None` when the src can't sit in that syntax or
/// is inline data (a pasted image), which is no use to read.
pub(super) fn image_markdown(attrs: &Value) -> Option<String> {
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

pub(super) fn is_list(n: &Value) -> bool {
    matches!(
        n["type"].as_str(),
        Some("bulletList" | "orderedList" | "taskList")
    )
}

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
