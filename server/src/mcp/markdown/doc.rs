use serde_json::Value;

use super::inline::mk_text;

pub(super) fn walk(node: &Value, f: &mut impl FnMut(&Value)) {
    f(node);
    for child in node["content"].as_array().into_iter().flatten() {
        walk(child, f);
    }
}

pub(super) fn walk_mut(node: &mut Value, f: &mut impl FnMut(&mut Value)) {
    f(node);
    if let Some(children) = node.get_mut("content").and_then(Value::as_array_mut) {
        for child in children {
            walk_mut(child, f);
        }
    }
}

/// Replace each chip that has no id with its label as plain text, then merge the
/// adjacent unmarked text runs that leaves.
pub(super) fn drop_unresolved(node: &mut Value) {
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

// ---------------------------------------------------------------- doc -> markdown

pub(super) fn is_chip(n: &Value) -> bool {
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
