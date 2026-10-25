use super::*;

#[tokio::test]
async fn query_text_location_reports_whole_blocks_and_the_version() {
    let f = fixture().await;
    f.k.create_note("Link", "", None).await.unwrap();
    f.k.create_note(
        "Subject",
        "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]].",
        None,
    )
    .await
    .unwrap();
    let note = f.k.get_note(NoteKey::Name("Subject")).await.unwrap();
    assert_eq!(
        body(&note),
        "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]]."
    );
    let v = version(&note);

    let found =
        f.k.query_text_location(NoteKey::Name("Subject"), "two")
            .await
            .unwrap();
    assert_eq!(
        found,
        format!("version: {v}\n\nlines 5-6 (1 match):\n- one\n- two")
    );
    let found =
        f.k.query_text_location(NoteKey::Name("Subject"), "Alpha.\n\n## Heading")
            .await
            .unwrap();
    assert!(
        found.contains("lines 1-1 (1 match):\nAlpha.\n\nlines 3-3 (1 match):\n## Heading"),
        "{found}"
    );
    let found =
        f.k.query_text_location(NoteKey::Name("Subject"), "o")
            .await
            .unwrap();
    assert!(
        found.ends_with("lines 5-6 (2 matches):\n- one\n- two"),
        "{found}"
    );
    let found =
        f.k.query_text_location(NoteKey::Name("Subject"), "n")
            .await
            .unwrap();
    let (a, b, c) = (
        found.find("lines 3-3").unwrap(),
        found.find("lines 5-6").unwrap(),
        found.find("lines 8-8").unwrap(),
    );
    assert!(a < b && b < c, "{found}");
    assert!(f
        .k
        .query_text_location(NoteKey::Name("Subject"), "nowhere in the note")
        .await
        .unwrap()
        .starts_with("No match"));
    assert!(
        err(f.k.query_text_location(NoteKey::Name("Subject"), "").await).contains("query is empty")
    );
}

#[tokio::test]
async fn edit_note_replaces_inserts_and_deletes_whole_blocks() {
    let f = fixture().await;
    f.k.create_note("Link", "", None).await.unwrap();
    f.k.create_note(
        "Subject",
        "Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[Link]].",
        None,
    )
    .await
    .unwrap();
    let id = f.note_id("Subject").await;
    let read = || async {
        let note = f.k.get_note(NoteKey::Name("Subject")).await.unwrap();
        (body(&note).to_string(), version(&note))
    };

    let (_, v) = read().await;
    f.k.edit_note(NoteKey::Name("Subject"), 3, 3, "## Renamed heading", v)
        .await
        .unwrap();
    let (text, v) = read().await;
    assert_eq!(
        text,
        "Alpha.\n\n## Renamed heading\n\n- one\n- two\n\nLast [[Link]]."
    );

    let stale = err(f
        .k
        .edit_note(NoteKey::Name("Subject"), 1, 1, "Stale", v - 1)
        .await);
    assert!(stale.contains("changed since you read"), "{stale}");
    assert!(!read().await.0.contains("Stale"));

    let inside_start = err(f.k.edit_note(NoteKey::Name("Subject"), 6, 6, "x", v).await);
    assert!(
        inside_start.contains("line 6 is inside the block at lines 5-6")
            && inside_start.contains("start at 5"),
        "{inside_start}"
    );
    let inside_end = err(f.k.edit_note(NoteKey::Name("Subject"), 5, 5, "x", v).await);
    assert!(inside_end.contains("end at 6"), "{inside_end}");
    assert!(
        err(f.k.edit_note(NoteKey::Name("Subject"), 2, 2, "x", v).await)
            .contains("contain no block")
    );
    assert_eq!(read().await.0, text, "a rejected range writes nothing");

    f.k.edit_note(NoteKey::Name("Subject"), 1, 0, "First.", v)
        .await
        .unwrap();
    let (text, v) = read().await;
    assert!(
        text.starts_with("First.\n\nAlpha.\n\n## Renamed heading"),
        "{text}"
    );

    // Blank lines at either edge of a range are fine.
    f.k.edit_note(NoteKey::Name("Subject"), 2, 4, "Alpha two.", v)
        .await
        .unwrap();
    let (text, v) = read().await;
    assert!(
        text.starts_with("First.\n\nAlpha two.\n\n## Renamed heading"),
        "{text}"
    );

    // Now: First. 1, Alpha two. 3, heading 5, list 7-8, Last 10.
    assert_eq!(f.link_targets(&id).await.len(), 1);
    f.k.edit_note(NoteKey::Name("Subject"), 10, 10, "Last, rewritten.", v)
        .await
        .unwrap();
    assert!(
        f.link_targets(&id).await.is_empty(),
        "replacing the block that held a link drops the link"
    );

    let (_, v) = read().await;
    f.k.edit_note(NoteKey::Name("Subject"), 5, 8, "", v)
        .await
        .unwrap();
    let (text, v) = read().await;
    assert_eq!(text, "First.\n\nAlpha two.\n\nLast, rewritten.");

    f.k.edit_note(NoteKey::Name("Subject"), 6, 5, "Appended.", v)
        .await
        .unwrap();
    let (text, v) = read().await;
    assert_eq!(
        text,
        "First.\n\nAlpha two.\n\nLast, rewritten.\n\nAppended."
    );

    assert!(err(f
        .k
        .edit_note(NoteKey::Name("Subject"), 99, 99, "x", v)
        .await)
    .contains("start 99 is outside"));
    assert!(
        err(f.k.edit_note(NoteKey::Name("Subject"), 1, 99, "x", v).await)
            .contains("end 99 is outside")
    );
    assert!(
        err(f.k.edit_note(NoteKey::Name("Subject"), 0, 0, "x", v).await)
            .contains("start must be at least 1")
    );
    assert!(
        err(f.k.edit_note(NoteKey::Name("Subject"), 1, 1, "x", -1).await)
            .contains("version at least 0")
    );
    assert!(
        err(f.k.edit_note(NoteKey::Name("Subject"), 3, 1, "x", v).await)
            .contains("end 1 is outside")
    );

    f.k.edit_note(NoteKey::Name("Subject"), 1, 7, "", v)
        .await
        .unwrap();
    let (text, v) = read().await;
    assert_eq!(text, "", "deleting every block leaves an empty body");
    f.k.edit_note(NoteKey::Name("Subject"), 1, 0, "Fresh.", v)
        .await
        .unwrap();
    assert_eq!(read().await.0, "Fresh.");
    let (doc, _) =
        f.k.get_doc(&tools::doc_ref("Note", &id).unwrap())
            .await
            .unwrap();
    assert_eq!(
        doc["content"].as_array().unwrap().len(),
        1,
        "no stray empty paragraph"
    );
}

#[tokio::test]
async fn append_keeps_nodes_the_converter_does_not_know() {
    let f = fixture().await;
    f.k.create_note("Rich", "First.", None).await.unwrap();
    let id = f.note_id("Rich").await;
    let r = tools::doc_ref("Note", &id).unwrap();
    let (_, updated_at) = f.k.get_doc(&r).await.unwrap();
    let doc = json!({ "type": "doc", "content": [
        { "type": "paragraph", "content": [{ "type": "text", "text": "kept", "marks": [{ "type": "link", "attrs": { "href": "u" } }] }] },
        { "type": "horizontalRule" },
    ]});
    f.k.put_doc(&r, &doc, Some(updated_at)).await.unwrap();

    f.k.append_to_note("Rich", "Second.").await.unwrap();
    let (saved, _) = f.k.get_doc(&r).await.unwrap();
    assert_eq!(saved["content"][0], doc["content"][0]);
    assert_eq!(saved["content"][1], doc["content"][1]);
    assert_eq!(saved["content"][2]["content"][0]["text"], "Second.");
}

#[tokio::test]
async fn edit_note_keeps_an_image_it_inserts_beside() {
    let f = fixture().await;
    let image = json!({ "type": "image", "attrs": { "src": "data:image/png;base64,AAAA", "width": "40%" } });
    let r = note_with_blocks(&f, "Picture", json!([image])).await;
    let note = f.k.get_note(NoteKey::Name("Picture")).await.unwrap();
    assert_eq!(body(&note), "<!-- calcifer:image -->");

    f.k.edit_note(
        NoteKey::Name("Picture"),
        1,
        0,
        "Caption first.",
        version(&note),
    )
    .await
    .unwrap();
    let (doc, _) = f.k.get_doc(&r).await.unwrap();
    assert_eq!(doc["content"][0]["content"][0]["text"], "Caption first.");
    assert_eq!(doc["content"][1], image, "the image survives the insert");
}

#[tokio::test]
async fn edit_note_refuses_a_range_over_what_markdown_cannot_express() {
    let f = fixture().await;
    let table = json!({ "type": "table", "content": [] });
    let para =
        |text: &str| json!({ "type": "paragraph", "content": [{ "type": "text", "text": text }] });
    let r = note_with_blocks(
        &f,
        "Tabular",
        json!([para("Intro."), table, para("Outro.")]),
    )
    .await;
    let note = f.k.get_note(NoteKey::Name("Tabular")).await.unwrap();
    assert_eq!(body(&note), "Intro.\n\n<!-- calcifer:table -->\n\nOutro.");
    let (before, _) = f.k.get_doc(&r).await.unwrap();

    let refused = err(f
        .k
        .edit_note(
            NoteKey::Name("Tabular"),
            1,
            5,
            "Everything, rewritten.",
            version(&note),
        )
        .await);
    assert!(
        refused.contains(
            "lines 3-3 hold a table that markdown can't express; leave them out of the range"
        ),
        "{refused}"
    );
    assert_eq!(
        f.k.get_doc(&r).await.unwrap().0,
        before,
        "a refused edit writes nothing"
    );

    // Ranges beside it are fine.
    f.k.edit_note(
        NoteKey::Name("Tabular"),
        5,
        5,
        "Outro, rewritten.",
        version(&note),
    )
    .await
    .unwrap();
    let (after, _) = f.k.get_doc(&r).await.unwrap();
    assert_eq!(after["content"][1], table);
}

#[tokio::test]
async fn edit_note_refuses_formatting_markdown_cannot_express() {
    let f = fixture().await;
    let link = json!({ "type": "link", "attrs": {
        "href": "https://example.com", "target": "_blank", "rel": "noopener noreferrer", "class": null, "title": null,
    } });
    let r = note_with_blocks(&f, "Styled", json!([
        { "type": "paragraph", "attrs": { "textAlign": null }, "content": [
            { "type": "text", "text": "An " },
            { "type": "text", "text": "underlined", "marks": [{ "type": "underline" }] },
            { "type": "text", "text": " word." },
        ] },
        { "type": "heading", "attrs": { "textAlign": "center", "level": 2 }, "content": [{ "type": "text", "text": "Centred" }] },
        { "type": "paragraph", "attrs": { "textAlign": null }, "content": [
            { "type": "text", "text": "See " },
            { "type": "text", "text": "the site", "marks": [link] },
            { "type": "text", "text": "." },
        ] },
    ])).await;
    let note = f.k.get_note(NoteKey::Name("Styled")).await.unwrap();
    assert_eq!(
        body(&note),
        "An underlined word.\n\n## Centred\n\nSee [the site](https://example.com)."
    );
    let (before, _) = f.k.get_doc(&r).await.unwrap();

    let underline = err(f
        .k
        .edit_note(NoteKey::Name("Styled"), 1, 1, "Rewritten.", version(&note))
        .await);
    assert!(
        underline.contains("lines 1-1 hold underline formatting that markdown can't express"),
        "{underline}"
    );
    let centred = err(f
        .k
        .edit_note(
            NoteKey::Name("Styled"),
            3,
            3,
            "## Rewritten",
            version(&note),
        )
        .await);
    assert!(
        centred.contains("lines 3-3 hold a heading's textAlign that markdown can't express"),
        "{centred}"
    );
    assert_eq!(
        f.k.get_doc(&r).await.unwrap().0,
        before,
        "a refused edit writes nothing"
    );

    f.k.edit_note(
        NoteKey::Name("Styled"),
        5,
        5,
        "Now see [the docs](https://docs.rs/x_(y)).",
        version(&note),
    )
    .await
    .unwrap();
    let (after, _) = f.k.get_doc(&r).await.unwrap();
    let linked = &after["content"][2]["content"][1];
    assert_eq!(linked["text"], "the docs");
    assert_eq!(linked["marks"][0]["type"], "link");
    assert_eq!(linked["marks"][0]["attrs"]["href"], "https://docs.rs/x_(y)");
    assert_eq!(
        after["content"][0], before["content"][0],
        "the blocks before it are untouched"
    );
    assert_eq!(after["content"][1], before["content"][1]);
}

#[tokio::test]
async fn edit_note_by_date_refuses_a_stale_version() {
    let f = fixture().await;
    f.k.append_to_daily_note("2026-10-24", "First.")
        .await
        .unwrap();
    f.k.create_note("Subject", "First.", None).await.unwrap();
    let day = f.k.get_note(NoteKey::Date("2026-10-24")).await.unwrap();
    let note = f.k.get_note(NoteKey::Name("Subject")).await.unwrap();

    let stale = err(f
        .k
        .edit_note(
            NoteKey::Date("2026-10-24"),
            1,
            1,
            "Stale.",
            version(&day) - 1,
        )
        .await);
    assert!(
        stale.starts_with("Daily note \"October 24, 2026\" changed since you read it."),
        "{stale}"
    );
    let stale = err(f
        .k
        .edit_note(NoteKey::Name("Subject"), 1, 1, "Stale.", version(&note) - 1)
        .await);
    assert!(
        stale.starts_with("Note \"Subject\" changed since you read it."),
        "{stale}"
    );
    let day = f.k.get_note(NoteKey::Date("2026-10-24")).await.unwrap();
    assert_eq!(body(&day), "First.", "a refused edit writes nothing");
}
