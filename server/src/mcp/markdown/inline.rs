use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

// ---------------------------------------------------------------- markdown -> doc

// Dots only between characters (`#program.apollo`), so a sentence's closing
// period stays text: `#biology.` is the tag `biology`.
pub(super) static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#([\p{L}\p{N}_-]+(?:\.[\p{L}\p{N}_-]+)*)").unwrap());

pub(super) static ISO_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]{4})-([0-9]{2})-([0-9]{2})").unwrap());

/// The character a backslash at the start of `text` escapes: ASCII punctuation
/// or a digit. Before anything else a backslash is just a backslash.
pub(super) fn escaped_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    if chars.next() != Some('\\') {
        return None;
    }
    chars
        .next()
        .filter(|c| c.is_ascii_punctuation() || c.is_ascii_digit())
}

/// `\X` read as the literal X.
pub(super) fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < text.len() {
        let (c, width) = if let Some(c) = escaped_char(&text[i..]) {
            (c, 1 + c.len_utf8())
        } else {
            let c = text[i..].chars().next().expect("i < text.len()");
            (c, c.len_utf8())
        };
        out.push(c);
        i += width;
    }
    out
}

/// The byte offset of the first `delim` in `text` that no backslash escapes and
/// no link holds, so `*[a **b**](u)*` closes at its last star.
pub(super) fn find_delimiter(text: &str, delim: &str) -> Option<usize> {
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
pub(super) struct LinkSpan<'a> {
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
pub(super) fn link_at(text: &str) -> Option<LinkSpan<'_>> {
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

/// A mark markdown can write. Variants sit in the order the editor's schema
/// ranks them, which is the order it stores them on a text node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Mark {
    Link(String),
    Bold,
    Code,
    Italic,
    Strike,
}

/// The attributes the editor gives every link (`Link.configure` in
/// TiptapEditor.tsx); a link with any other values has no markdown form.
pub(super) const LINK_TARGET: &str = "_blank";

pub(super) const LINK_REL: &str = "noopener noreferrer";

impl Mark {
    pub(super) fn to_json(&self) -> Value {
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
    pub(super) fn from_json(mark: &Value) -> Option<Mark> {
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
pub(super) fn link_href(attrs: &Value) -> Option<&str> {
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

pub(super) fn mk_text(text: &str, marks: &[Mark]) -> Value {
    let mut node = json!({ "type": "text", "text": text });
    if !marks.is_empty() {
        node["marks"] = marks.iter().map(Mark::to_json).collect();
    }
    node
}

/// "yyyy-MM-dd" naming a real day: 2026-02-28 is one, 2026-02-30 and 2026-13-45 aren't.
pub(super) fn is_calendar_day(year: &str, month: &str, day: &str) -> bool {
    match (year.parse(), month.parse(), day.parse()) {
        (Ok(y), Ok(m), Ok(d)) => chrono::NaiveDate::from_ymd_opt(y, m, d).is_some(),
        _ => false,
    }
}

/// The ISO date `text` opens with, when it names a real day and no digit
/// follows (2026-06-155 is not a date): at a word boundary, a dateChip.
pub(super) fn date_at(text: &str) -> Option<&str> {
    let m = ISO_DATE.captures(text)?;
    let date = m.get(0)?.as_str();
    let glued = text[date.len()..].starts_with(|c: char| c.is_ascii_digit());
    (!glued && is_calendar_day(&m[1], &m[2], &m[3])).then_some(date)
}

/// Splits a text run into marked text, mention, hashtag, dateChip and hardBreak
/// nodes; a `\n` is a hard break (see `join_lines`). mention and hashtag carry a
/// temporary `_name` attr that `Parsed::resolve` turns into an id; a mention
/// written `[[Name|label]]` also carries `_alias`. `\X` is a
/// literal X. Inline code is verbatim: no chips, escapes or emphasis inside it.
/// A link's text is parsed like any other run, but holds no link of its own.
pub(super) fn parse_inline(text: &str, marks: &[Mark]) -> Vec<Value> {
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
                            "_name": name,
                            // The link as written, for `Parsed::aliases`.
                            "_alias": pipe.map(|_| &rest[..2 + end + 2]),
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
pub(super) fn with_mark(marks: &[Mark], mark: Mark) -> Vec<Mark> {
    let mut out = marks.to_vec();
    if !out.contains(&mark) {
        out.push(mark);
        out.sort();
    }
    out
}
