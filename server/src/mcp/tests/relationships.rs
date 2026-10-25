use super::*;

#[tokio::test]
async fn get_backlinks_missing_note_is_an_error() {
    let f = fixture().await;
    let before = f.entity_count().await;

    let reply = handle_message(
        &f.k,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "get_backlinks", "arguments": { "name": "Ghost" } } }),
    )
    .await
    .unwrap();
    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(
        reply["result"]["content"][0]["text"],
        "No note named \"Ghost\". Use search_notes or create_note."
    );
    assert_eq!(f.entity_count().await, before, "nothing was created");
}

#[tokio::test]
async fn link_notes_relates_without_writing_text() {
    let f = fixture().await;
    f.k.create_note("A", "Body A.", None).await.unwrap();
    f.k.create_note("B", "Body B.", None).await.unwrap();
    let before = f.k.get_note(NoteKey::Name("A")).await.unwrap();

    assert_eq!(
        f.k.link_notes("A", "B").await.unwrap(),
        "Related \"A\" -> \"B\"."
    );
    let after = f.k.get_note(NoteKey::Name("A")).await.unwrap();
    assert_eq!(body(&after), "Body A.", "{after}");
    assert_eq!(version(&after), version(&before), "no text was saved");
    assert!(
        after.contains("\ntags: []\nrelated: [\"B\"]\n---\n"),
        "{after}"
    );
    let b = f.k.get_note(NoteKey::Name("B")).await.unwrap();
    assert!(b.contains("\nrelated: []\n"), "one-way: {b}");

    // Names match case-insensitively; the message uses the stored names.
    assert_eq!(
        f.k.link_notes("a", "b").await.unwrap(),
        "\"A\" was already related to \"B\"."
    );
    let again = f.k.get_note(NoteKey::Name("A")).await.unwrap();
    assert_eq!(body(&again), "Body A.");
    assert!(again.contains("\nrelated: [\"B\"]\n"), "{again}");
}

#[tokio::test]
async fn get_note_lists_related_oldest_first() {
    let f = fixture().await;
    for name in ["A", "B", "C"] {
        f.k.create_note(name, "", None).await.unwrap();
    }
    f.k.link_notes("A", "C").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    f.k.link_notes("A", "B").await.unwrap();

    let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
    assert!(note.contains("\nrelated: [\"C\",\"B\"]\n"), "{note}");

    f.k.unlink_notes("A", "C").await.unwrap();
    let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
    assert!(note.contains("\nrelated: [\"B\"]\n"), "{note}");
}

#[tokio::test]
async fn unlink_notes_removes_the_relationship() {
    let f = fixture().await;
    f.k.create_note("A", "Body A.", None).await.unwrap();
    f.k.create_note("B", "Body B.", None).await.unwrap();
    f.k.link_notes("A", "B").await.unwrap();

    assert_eq!(
        f.k.unlink_notes("A", "B").await.unwrap(),
        "Unrelated \"A\" -> \"B\"."
    );
    let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
    assert!(note.contains("\nrelated: []\n"), "{note}");
    assert_eq!(body(&note), "Body A.");
    assert!(f.link_targets(&f.note_id("A").await).await.is_empty());

    assert_eq!(
        f.k.unlink_notes("A", "B").await.unwrap(),
        "\"A\" wasn't related to \"B\"."
    );
    assert_eq!(
        f.k.unlink_notes("b", "a").await.unwrap(),
        "\"B\" wasn't related to \"A\".",
        "the other direction was never related"
    );
}

#[tokio::test]
async fn link_notes_takes_an_id_and_refuses_an_ambiguous_name() {
    let f = fixture().await;
    f.k.create_note("A", "", None).await.unwrap();
    f.create("Note", "Dup").await;
    f.create("Note", "Dup").await;
    let dups = f.notes_named("Dup").await;
    assert_eq!(dups.len(), 2);

    for failure in [
        err(f.k.link_notes("A", "dup").await),
        err(f.k.link_notes("Dup", "A").await),
        err(f.k.unlink_notes("A", "Dup").await),
    ] {
        assert!(failure.starts_with("Several notes are named"), "{failure}");
        for id in &dups {
            assert!(failure.contains(&format!("- Dup ({id})")), "{failure}");
        }
        assert!(failure.ends_with("Call again with the id."), "{failure}");
    }
    assert!(f.link_targets(&f.note_id("A").await).await.is_empty());

    assert_eq!(
        f.k.link_notes("A", &dups[0]).await.unwrap(),
        "Related \"A\" -> \"Dup\"."
    );
    assert_eq!(
        f.link_targets(&f.note_id("A").await).await,
        [format!("Note:{}", dups[0])]
    );
    assert_eq!(
        f.k.link_notes(&dups[1], "A").await.unwrap(),
        "Related \"Dup\" -> \"A\"."
    );
    assert_eq!(
        f.k.unlink_notes("A", &dups[0]).await.unwrap(),
        "Unrelated \"A\" -> \"Dup\"."
    );
}

#[tokio::test]
async fn link_notes_missing_note_is_an_error() {
    let f = fixture().await;
    f.k.create_note("A", "", None).await.unwrap();
    f.create("Tag", "ml").await;
    let tag = tag_id(&f, "ml").await;
    let before = f.entity_count().await;

    for failure in [
        err(f.k.link_notes("A", "Ghost").await),
        err(f.k.link_notes("Ghost", "A").await),
        err(f.k.unlink_notes("A", "Ghost").await),
        err(f.k.unlink_notes("Ghost", "A").await),
        err(f.k.link_notes("A", &uuid::Uuid::new_v4().to_string()).await),
        err(f.k.link_notes("A", "[[A]]").await),
    ] {
        assert!(
            failure.starts_with("No note named or with id \"")
                && failure.ends_with("\". Use search_notes."),
            "{failure}"
        );
    }
    assert!(
        err(f.k.link_notes("A", &tag).await).starts_with("No note named or with id"),
        "the id of a Tag isn't a note's"
    );
    assert!(
        err(f.k.link_notes("A", "ml").await).starts_with("No note named or with id"),
        "a Tag's name isn't a note's"
    );
    assert_eq!(f.entity_count().await, before, "nothing is created");
    let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
    assert!(note.contains("\nrelated: []\n"), "{note}");
}

#[tokio::test]
async fn link_notes_refuses_a_note_related_to_itself() {
    let f = fixture().await;
    f.k.create_note("A", "", None).await.unwrap();
    let failure = err(f.k.link_notes("A", "a").await);
    assert!(failure.contains("itself"), "{failure}");
}

#[tokio::test]
async fn get_backlinks_labels_related_sources() {
    let f = fixture().await;
    f.k.create_note("B", "", None).await.unwrap();
    f.k.create_note("A", "", None).await.unwrap();
    f.k.create_note("Mentioner", "Points at [[B]].", None)
        .await
        .unwrap();
    f.k.link_notes("A", "B").await.unwrap();

    let backlinks = f.k.get_backlinks("B").await.unwrap();
    let mut lines: Vec<&str> = backlinks.lines().collect();
    lines.sort_unstable();
    assert_eq!(lines, ["- A (related)", "- Mentioner"], "{backlinks}");

    // A mention and a relationship from one source is one line.
    f.k.append_to_note("A", "Also [[B]].").await.unwrap();
    let backlinks = f.k.get_backlinks("B").await.unwrap();
    assert_eq!(
        backlinks.lines().filter(|l| l.starts_with("- A")).count(),
        1,
        "{backlinks}"
    );
    assert!(backlinks.contains("- A (related)"), "{backlinks}");

    f.k.unlink_notes("A", "B").await.unwrap();
    let backlinks = f.k.get_backlinks("B").await.unwrap();
    assert!(!backlinks.contains("related"), "{backlinks}");
    assert!(
        backlinks.contains("- A\n") || backlinks.ends_with("- A"),
        "{backlinks}"
    );
}

#[tokio::test]
async fn create_note_ignores_frontmatter_related() {
    let f = fixture().await;
    f.k.create_note("A", "Body.", None).await.unwrap();
    f.k.create_note("B", "", None).await.unwrap();
    f.k.link_notes("A", "B").await.unwrap();
    let note = f.k.get_note(NoteKey::Name("A")).await.unwrap();
    assert!(note.contains("\nrelated: [\"B\"]\n"), "{note}");

    f.k.create_note("A Copy", &note, None).await.unwrap();
    let copy = f.k.get_note(NoteKey::Name("A Copy")).await.unwrap();
    assert_eq!(body(&copy), "Body.", "{copy}");
    assert!(copy.contains("\nrelated: []\n"), "{copy}");
    assert!(f.link_targets(&f.note_id("A Copy").await).await.is_empty());
    assert_eq!(
        f.link_targets(&f.note_id("A").await).await.len(),
        1,
        "the original keeps its relationship"
    );
}
