//! I-82: two notes can share a name (the browser makes one freely). Every tool
//! reaches either by id, and refuses the bare name, listing both, with nothing
//! written.

use super::*;

/// Two notes named Mercury: the planet, with a body, and the god, empty. The
/// god is made as the browser makes a note, since create_note refuses the name.
async fn two_mercuries(f: &Fixture) -> (String, String) {
    f.k.create_note("Mercury", "The planet.", None)
        .await
        .unwrap();
    f.create("Note", "MERCURY").await;
    let planet = f.notes_named("Mercury").await.remove(0);
    let god = f.notes_named("MERCURY").await.remove(0);
    (planet, god)
}

fn refused_listing(failure: &str, ids: &[&String]) {
    for id in ids {
        assert!(failure.contains(&format!("({id}), created ")), "{failure}");
    }
}

#[tokio::test]
async fn every_tool_refuses_a_shared_name() {
    let f = fixture().await;
    let (planet, god) = two_mercuries(&f).await;
    let version = version(&f.k.get_note(NoteKey::Name(&planet)).await.unwrap());
    let before = f.entity_count().await;

    for failure in [
        err(f.k.get_note(NoteKey::Name("mercury")).await),
        err(f
            .k
            .query_text_location(NoteKey::Name("Mercury"), "planet")
            .await),
        err(f
            .k
            .edit_note(NoteKey::Name("Mercury"), 1, 1, "Gone.", version)
            .await),
        err(f.k.append_to_note("Mercury", "More.").await),
        err(f.k.tag_entity("Mercury", &tag_args(&["ml"]), &[]).await),
        err(f.k.get_backlinks("Mercury").await),
    ] {
        assert!(
            failure.starts_with("Several notes are named \""),
            "{failure}"
        );
        assert!(failure.ends_with("Call again with the id."), "{failure}");
        refused_listing(&failure, &[&planet, &god]);
    }

    let read = f.k.get_note(NoteKey::Name(&planet)).await.unwrap();
    assert_eq!(body(&read), "The planet.");
    assert_eq!(f.entity_count().await, before, "no Tag was created");
    assert!(f.link_targets(&planet).await.is_empty());
    assert!(f.link_targets(&god).await.is_empty());
}

#[tokio::test]
async fn every_tool_reaches_the_second_by_id() {
    let f = fixture().await;
    let (planet, god) = two_mercuries(&f).await;

    let appended = f.k.append_to_note(&god, "The god.").await.unwrap();
    assert!(
        appended.contains(&format!("\"MERCURY\" ({god})")),
        "{appended}"
    );
    let read = f.k.get_note(NoteKey::Name(&god)).await.unwrap();
    assert_eq!(body(&read), "The god.");
    assert!(read.contains(&format!("\nid: \"{god}\"\n")), "{read}");

    let found =
        f.k.query_text_location(NoteKey::Name(&god), "god")
            .await
            .unwrap();
    assert!(found.contains("lines 1-1"), "{found}");
    f.k.edit_note(
        NoteKey::Name(&god),
        1,
        1,
        "The messenger god.",
        version(&read),
    )
    .await
    .unwrap();
    f.k.tag_entity(&god, &tag_args(&["myth"]), &[])
        .await
        .unwrap();
    assert_eq!(
        f.k.get_backlinks(&god).await.unwrap(),
        "Nothing links to \"MERCURY\" yet."
    );

    let god_note = f.k.get_note(NoteKey::Name(&god)).await.unwrap();
    assert_eq!(body(&god_note), "The messenger god.");
    assert!(god_note.contains("\ntags: [\"myth\"]\n"), "{god_note}");
    let planet_note = f.k.get_note(NoteKey::Name(&planet)).await.unwrap();
    assert_eq!(body(&planet_note), "The planet.");
    assert!(planet_note.contains("\ntags: []\n"), "{planet_note}");
}

#[tokio::test]
async fn a_link_to_a_shared_name_is_refused_and_one_by_id_is_kept() {
    let f = fixture().await;
    let (planet, god) = two_mercuries(&f).await;
    let before = f.entity_count().await;

    for failure in [
        err(f.k.create_note("Index", "See [[Mercury]].", None).await),
        err(f.k.append_to_note(&planet, "See [[mercury]].").await),
        err(f
            .k
            .append_to_daily_note("2026-11-04", "See [[Mercury]].")
            .await),
    ] {
        assert!(failure.starts_with("Nothing was written"), "{failure}");
        assert!(
            failure.contains("[[Mercury]] names:") || failure.contains("[[mercury]] names:"),
            "{failure}"
        );
        assert!(failure.ends_with("as [[<id>]]."), "{failure}");
        refused_listing(&failure, &[&planet, &god]);
    }
    assert_eq!(f.entity_count().await, before, "no note or day was created");
    assert_eq!(
        body(&f.k.get_note(NoteKey::Name(&planet)).await.unwrap()),
        "The planet."
    );

    f.k.create_note("Index", &format!("See [[{god}]]."), None)
        .await
        .unwrap();
    f.k.link_notes("Index", &god).await.unwrap();
    let index = f.note_id("Index").await;
    assert_eq!(
        f.link_targets(&index).await,
        [format!("Note:{god}"), format!("Note:{god}")]
    );
    // While the name is shared, the link and `related` are written by id, which
    // reads back as the same note.
    let read = f.k.get_note(NoteKey::Name("Index")).await.unwrap();
    assert_eq!(body(&read), format!("See [[{god}]]."));
    assert!(
        read.contains(&format!("\nrelated: [\"{god}\"]\n")),
        "{read}"
    );
    f.k.edit_note(NoteKey::Name("Index"), 1, 1, body(&read), version(&read))
        .await
        .unwrap();

    // Once the planet is renamed, the god's name is its own again.
    f.entity
        .rename_entity(Request::new(RenameEntityRequest {
            id: planet.clone(),
            name: "Mercury (planet)".into(),
        }))
        .await
        .unwrap();
    let read = f.k.get_note(NoteKey::Name("Index")).await.unwrap();
    assert_eq!(body(&read), "See [[MERCURY]].");
    assert!(read.contains("\nrelated: [\"MERCURY\"]\n"), "{read}");
}

#[tokio::test]
async fn create_note_lists_the_notes_that_have_the_name() {
    let f = fixture().await;
    let (planet, god) = two_mercuries(&f).await;
    let failure = err(f.k.create_note("mercury", "A third.", None).await);
    assert!(
        failure.contains("already exists; nothing was written"),
        "{failure}"
    );
    refused_listing(&failure, &[&planet, &god]);
    assert_eq!(f.notes_named("mercury").await.len(), 0);
}

#[tokio::test]
async fn search_results_carry_ids() {
    let f = fixture().await;
    let (planet, god) = two_mercuries(&f).await;
    let found =
        f.k.search_notes("Mercury", 10, SearchMode::Lexical)
            .await
            .unwrap();
    assert!(
        found.contains(&format!("[Note] Mercury ({planet})")),
        "{found}"
    );
    assert!(
        found.contains(&format!("[Note] MERCURY ({god})")),
        "{found}"
    );
}
