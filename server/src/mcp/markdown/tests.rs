use super::*;
use super::{doc::*, expressible::*, inline::*, write::*};

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
        "Ship by [[DailyNote/June 13, 2026]]; see [[Tag/ml]].",
        "A note named [[Note/Tag/ml]] and one named [[Foo/Bar]].",
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

#[allow(
    clippy::needless_pass_by_value,
    reason = "call sites pass json! literals"
)]
fn text(text: &str, marks: Value) -> Value {
    json!({ "type": "text", "text": text, "marks": marks })
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "call sites pass json! literals"
)]
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
fn a_tag_may_hold_dots_but_not_end_in_one() {
    let labels = |md: &str| -> Vec<Value> {
        chips(&to_tt(md))
            .iter()
            .map(|c| c["label"].clone())
            .collect()
    };
    assert_eq!(labels("See #program.apollo now"), [json!("program.apollo")]);
    assert_eq!(labels("Ship #v1.2."), [json!("v1.2")]);
    assert_eq!(labels("Ends #biology."), [json!("biology")]);
    assert_eq!(labels("Two #a..b"), [json!("a")]);
}

#[test]
fn structure_prefix_targets_only_registry_types() {
    let tag = &chips(&to_tt("[[Tag/ship]]"))[0];
    assert_eq!(
        (&tag["structureType"], &tag["id"], &tag["label"]),
        (&json!("Tag"), &json!("Tag:ship"), &json!("ship"))
    );
    let foo = &chips(&to_tt("[[Foo/Bar]]"))[0];
    assert_eq!(
        (&foo["structureType"], &foo["id"]),
        (&json!("Note"), &json!("Note:Foo/Bar"))
    );
    let lower = &chips(&to_tt("[[tag/ship]]"))[0];
    assert_eq!(
        lower["id"], "Note:tag/ship",
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
    let parsed = parse("[[Attention]] and [[attention]] and [[Tag/ship]]");
    let keys: Vec<_> = parsed
        .references
        .iter()
        .map(|r| (r.structure_type.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(keys, [("Note", "Attention"), ("Tag", "ship")]);
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
