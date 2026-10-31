use super::*;

#[tokio::test]
async fn a_note_with_a_link_and_a_tag_derives_two_links_and_backlinks() {
    let f = fixture().await;
    f.k.create_note("Attention", "", None).await.unwrap();
    f.k.create_note("Positional Encoding", "", None)
        .await
        .unwrap();
    let saved =
        f.k.create_note(
            "Transformers",
            "An architecture built on [[Attention]]. #ml",
            None,
        )
        .await
        .unwrap();
    assert!(
        saved.contains("Wikilinks: 2 [") && saved.contains("Note") && saved.contains("Tag"),
        "{saved}"
    );

    assert_eq!(
        f.k.get_backlinks("Attention").await.unwrap(),
        "- Transformers"
    );
    assert_eq!(
        f.k.get_backlinks("Transformers").await.unwrap(),
        "Nothing links to \"Transformers\" yet."
    );
    assert_eq!(
        err(f.k.get_backlinks("Nowhere").await),
        "No note named \"Nowhere\". Use search_notes or create_note."
    );

    let note = f.k.get_note(NoteKey::Name("Transformers")).await.unwrap();
    assert!(
        note.contains("[[Attention]]") && note.contains("#ml"),
        "{note}"
    );

    f.k.append_to_note("Transformers", "They use [[Positional Encoding]] too.")
        .await
        .unwrap();
    let note = f.k.get_note(NoteKey::Name("Transformers")).await.unwrap();
    assert!(
        note.contains("[[Positional Encoding]]") && note.contains("[[Attention]]"),
        "{note}"
    );
    assert_eq!(
        f.k.get_backlinks("Positional Encoding").await.unwrap(),
        "- Transformers"
    );
}

#[tokio::test]
async fn writing_to_a_missing_note_is_an_error_and_creates_nothing() {
    let f = fixture().await;
    let before = f.entity_count().await;
    for failure in [
        err(f.k.append_to_note("Ghost", "x").await),
        err(f.k.link_notes("Ghost", "Attention").await),
        err(f.k.query_text_location(NoteKey::Name("Ghost"), "x").await),
        err(f.k.edit_note(NoteKey::Name("Ghost"), 1, 1, "x", 0).await),
    ] {
        assert!(failure.contains("No note named"), "{failure}");
    }
    assert_eq!(f.entity_count().await, before);
}

#[tokio::test]
async fn get_note_writes_chips_from_the_targets_current_names() {
    let f = fixture().await;
    f.k.create_note("Old Name", "", None).await.unwrap();
    f.k.create_note("Source", "Points at [[Old Name]].", None)
        .await
        .unwrap();
    let id = f.note_id("Old Name").await;
    let rename = RenameEntityRequest {
        id,
        name: "New Name".into(),
    };
    f.entity.rename_entity(Request::new(rename)).await.unwrap();
    let note = f.k.get_note(NoteKey::Name("Source")).await.unwrap();
    assert!(
        note.contains("[[New Name]]") && !note.contains("Old Name"),
        "{note}"
    );
}

#[tokio::test]
async fn get_note_copied_into_create_note_links_the_same_targets() {
    let f = fixture().await;
    f.create("Tag", "ship").await;
    f.k.create_daily_note("2026-06-13").await.unwrap();
    f.k.create_note("RT Note", "", None).await.unwrap();
    f.k.create_note("RT Alias", "", None).await.unwrap();
    let md = "Uses [[RT Note]] and [[RT Alias]]. #rt\n\n- do [[Tag/ship]] on [[DailyNote/June 13, 2026]]";
    let tags = ["rt-a".to_string(), "rt-b".to_string()];
    f.k.create_note("Round Trip", md, Some(&tags))
        .await
        .unwrap();
    let id = f.note_id("Round Trip").await;
    let links_before = f.link_targets(&id).await;
    // Five from the body, two from `tags`.
    assert_eq!(links_before.len(), 7, "{links_before:?}");
    assert!(
        links_before.iter().any(|l| l.starts_with("Tag:"))
            && links_before.iter().any(|l| l.starts_with("DailyNote:"))
    );

    let count = f.entity_count().await;
    let note = f.k.get_note(NoteKey::Name("Round Trip")).await.unwrap();
    assert!(
        note.contains("[[Tag/ship]]") && note.contains("[[DailyNote/June 13, 2026]]"),
        "{note}"
    );
    assert!(
        note.contains("[[RT Alias]]") && note.contains("#rt"),
        "{note}"
    );
    assert!(
        note.starts_with("---\nname: \"Round Trip\"\nversion: "),
        "{note}"
    );
    let frontmatter = note
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .unwrap();
    let lines: Vec<&str> = frontmatter.lines().collect();
    assert!(
        lines.len() == 4
            && lines[0].starts_with("name: ")
            && lines[1].starts_with("version: ")
            && lines[2] == "tags: [\"rt-a\",\"rt-b\"]"
            && lines[3] == "related: []",
        "{note}"
    );

    // The whole result, frontmatter included, goes into a new note.
    f.k.create_note("Round Trip Copy", &note, None)
        .await
        .unwrap();
    assert_eq!(
        body(
            &f.k.get_note(NoteKey::Name("Round Trip Copy"))
                .await
                .unwrap()
        ),
        body(&note)
    );
    assert_eq!(
        f.link_targets(&f.note_id("Round Trip Copy").await).await,
        links_before
    );
    let source = f.k.entity(&id).await.unwrap();
    let copy =
        f.k.entity(&f.note_id("Round Trip Copy").await)
            .await
            .unwrap();
    assert_eq!(tag_ids(&copy), tag_ids(&source));
    assert_eq!(tag_ids(&copy).len(), 2, "the copy keeps the source's tags");
    assert_eq!(
        f.entity_count().await,
        count + 1,
        "only the copy itself is created"
    );
}

#[tokio::test]
async fn get_note_frontmatter_has_name_version_tags_and_related() {
    let f = fixture().await;
    f.k.create_note("Target", "Body.", None).await.unwrap();
    f.k.create_note("Source", "Points at [[Target]].", None)
        .await
        .unwrap();

    let note = f.k.get_note(NoteKey::Name("Target")).await.unwrap();
    let frontmatter = note
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .unwrap();
    let keys: Vec<&str> = frontmatter
        .lines()
        .map(|l| l.split_once(':').unwrap().0)
        .collect();
    assert_eq!(keys, ["name", "version", "tags", "related"], "{note}");

    let backlinks = f.k.get_backlinks("Target").await.unwrap();
    assert!(backlinks.contains("Source"), "{backlinks}");
}

#[tokio::test]
async fn create_note_refuses_an_existing_note() {
    let f = fixture().await;
    f.k.create_note("Existing", "Original.", None)
        .await
        .unwrap();
    let refused = err(f.k.create_note("Existing", "Replacement.", None).await);
    assert!(refused.contains("already exists"), "{refused}");
    assert!(
        err(f.k.create_note("existing", "Case.", None).await).contains("already exists"),
        "names match case-insensitively"
    );
    assert_eq!(
        body(&f.k.get_note(NoteKey::Name("Existing")).await.unwrap()),
        "Original."
    );

    f.k.create_note("Blank", "", None).await.unwrap();
    assert!(
        err(f.k.create_note("Blank", "Filled.", None).await).contains("already exists"),
        "even a blank note exists"
    );
}

// I-61: `[[Name|Alias]]` and `]]` can't be told from a name containing them.
#[tokio::test]
async fn create_note_refuses_a_pipe_in_the_name() {
    let f = fixture().await;
    let before = f.entity_count().await;

    for name in ["A|B", "x]]y"] {
        let refused = err(f.k.create_note(name, "Body.", None).await);
        assert_eq!(refused, r#"names can't contain "|" or "]]""#);
    }
    assert_eq!(f.entity_count().await, before, "no note was created");
    assert!(f.notes_named("A|B").await.is_empty());

    // A lone bracket is a fine name.
    f.k.create_note("a[b]c", "Body.", None).await.unwrap();
    assert_eq!(f.notes_named("a[b]c").await.len(), 1);
}

// The name is refused before resolving the body's #tags and the `tags` list,
// which would create Tags and leave them behind.
#[tokio::test]
async fn create_note_with_a_refused_name_leaves_nothing_behind() {
    let f = fixture().await;
    let before = f.entity_count().await;

    let refused = err(f
        .k
        .create_note("A|B", "Body #leaky.", Some(&["t1".into()]))
        .await);

    assert_eq!(refused, r#"names can't contain "|" or "]]""#);
    assert_eq!(f.entity_count().await, before, "no note or Tag was created");
}

// A note's name can look like a structure path: `[[Note/Tag/X]]` is the Note
// named "Tag/X", never the Tag "X", and link_notes takes the name as it is.
#[tokio::test]
async fn a_note_named_like_a_structure_path_round_trips() {
    let f = fixture().await;
    f.create("Tag", "X").await;
    f.k.create_note("Tag/X", "The note.", None).await.unwrap();
    let target = f.note_id("Tag/X").await;
    f.k.create_note("Source", "See [[Note/Tag/X]].", None)
        .await
        .unwrap();
    f.k.create_note("Third", "", None).await.unwrap();

    let source = f.k.get_note(NoteKey::Name("Source")).await.unwrap();
    assert_eq!(body(&source), "See [[Note/Tag/X]].", "{source}");
    assert_eq!(
        f.link_targets(&f.note_id("Source").await).await,
        [format!("Note:{target}")]
    );

    assert_eq!(
        f.k.link_notes("Third", "Tag/X").await.unwrap(),
        "Related \"Third\" -> \"Tag/X\"."
    );
    assert_eq!(
        f.link_targets(&f.note_id("Third").await).await,
        [format!("Note:{target}")]
    );
}

#[tokio::test]
async fn a_link_to_a_missing_target_fails_the_write_and_creates_nothing() {
    let f = fixture().await;
    f.k.create_note("Existing", "Body.", None).await.unwrap();
    let before = f.entity_count().await;
    let md = "See [[Attenton]] and [[DailyNote/February 1, 1901]].";
    for failure in [
        err(f.k.create_note("New", md, None).await),
        err(f.k.append_to_note("Existing", md).await),
        err(f.k.append_to_daily_note("2026-06-13", md).await),
        err(f
            .k
            .edit_note(
                NoteKey::Name("Existing"),
                1,
                1,
                md,
                version(&f.k.get_note(NoteKey::Name("Existing")).await.unwrap()),
            )
            .await),
    ] {
        assert!(
            failure.contains("Nothing was written: 2 link(s)"),
            "{failure}"
        );
        assert!(
            failure.contains("[[Attenton]], [[DailyNote/February 1, 1901]]"),
            "{failure}"
        );
    }
    assert_eq!(
        f.entity_count().await,
        before,
        "not even append_to_daily_note's day"
    );
    assert_eq!(
        body(&f.k.get_note(NoteKey::Name("Existing")).await.unwrap()),
        "Body."
    );
    assert!(err(f.k.get_note(NoteKey::Name("New")).await).starts_with("No note named"));
}

// I-84: `[[Name|label]]` is no feature; the write is refused and says so.
#[tokio::test]
async fn an_aliased_link_is_refused_by_every_write_tool() {
    let f = fixture().await;
    f.k.create_note("Existing", "Body.", None).await.unwrap();
    f.k.create_note("Apollo 11", "", None).await.unwrap();
    let before = f.entity_count().await;
    let existing = f.k.get_note(NoteKey::Name("Existing")).await.unwrap();
    let md = "Landed: [[Apollo 11|the first landing]]. Also `[[X|y]]`.";
    for failure in [
        err(f.k.create_note("New", md, None).await),
        err(f.k.append_to_note("Existing", md).await),
        err(f.k.append_to_daily_note("2026-06-13", md).await),
        err(f
            .k
            .edit_note(NoteKey::Name("Existing"), 1, 1, md, version(&existing))
            .await),
    ] {
        assert!(failure.starts_with("Nothing was written: "), "{failure}");
        assert!(
            failure.contains("[[Apollo 11|the first landing]]")
                && failure.contains("aliases aren't supported")
                && failure.contains("[[Apollo 11]]"),
            "{failure}"
        );
        assert!(!failure.contains("X|y"), "{failure}");
    }
    assert_eq!(f.entity_count().await, before, "not even the day");
    assert_eq!(
        f.k.get_note(NoteKey::Name("Existing")).await.unwrap(),
        existing
    );
    assert!(err(f.k.get_note(NoteKey::Name("New")).await).starts_with("No note named"));
}

#[tokio::test]
async fn a_pipe_in_code_is_not_an_alias() {
    let f = fixture().await;
    f.k.create_note("Apollo 11", "", None).await.unwrap();
    f.k.create_note(
        "Code",
        "Inline `[[Apollo 11|x]]` here.\n\n```\n[[Apollo 11|y]]\n```\n\nA | B and [[Apollo 11]].",
        None,
    )
    .await
    .unwrap();

    let note = f.k.get_note(NoteKey::Name("Code")).await.unwrap();

    assert!(body(&note).contains("`[[Apollo 11|x]]`"), "{note}");
    assert!(body(&note).contains("[[Apollo 11|y]]\n```"), "{note}");
}

#[tokio::test]
async fn create_note_may_link_to_itself() {
    let f = fixture().await;
    f.k.create_note("Selfie", "I am [[Selfie]].", None)
        .await
        .unwrap();
    let id = f.note_id("Selfie").await;
    assert_eq!(
        body(&f.k.get_note(NoteKey::Name("Selfie")).await.unwrap()),
        "I am [[Selfie]]."
    );
    assert_eq!(f.link_targets(&id).await, [format!("Note:{id}")]);
}

#[tokio::test]
async fn a_leading_rule_that_is_not_frontmatter_is_note_content() {
    let f = fixture().await;
    f.k.create_note("Rule", "---\nplain text\n---", None)
        .await
        .unwrap();
    assert!(body(&f.k.get_note(NoteKey::Name("Rule")).await.unwrap()).contains("plain text"));
}

#[tokio::test]
async fn get_note_missing_note_is_an_error() {
    let f = fixture().await;
    let before = f.entity_count().await;
    let call = |arguments: Value| {
        handle_message(
            &f.k,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "get_note", "arguments": arguments } }),
        )
    };

    for (arguments, expected) in [
        (
            json!({ "name": "Ghost" }),
            "No note named \"Ghost\". Use search_notes or create_note.",
        ),
        (
            json!({ "date": "2026-10-24" }),
            "No daily note for 2026-10-24. Use append_to_daily_note to start one.",
        ),
    ] {
        let reply = call(arguments.clone()).await.unwrap();
        assert!(reply.get("error").is_none(), "{arguments}: {reply}");
        assert_eq!(reply["result"]["isError"], true, "{arguments}: {reply}");
        assert_eq!(
            reply["result"]["content"][0]["text"], expected,
            "{arguments}"
        );
    }
    assert_eq!(f.entity_count().await, before, "nothing was created");
}
