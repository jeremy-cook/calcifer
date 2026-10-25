use super::*;

#[tokio::test]
async fn a_missing_tag_is_created() {
    let f = fixture().await;
    let saved =
        f.k.create_note("Tagged", "About #brandnew things.", None)
            .await
            .unwrap();
    assert!(saved.contains("Wikilinks: 1 [Tag]"), "{saved}");
}

#[tokio::test]
async fn create_note_with_tags_sets_the_tags_property() {
    let f = fixture().await;
    f.create("Tag", "ml").await;
    let tags = ["ml".to_string(), "brandnew".to_string(), "ML".to_string()];
    let saved =
        f.k.create_note("Tagged", "Body.", Some(&tags))
            .await
            .unwrap();
    assert!(
        saved.contains("Wikilinks: 0 [none]") && saved.contains("tags: 2."),
        "{saved}"
    );

    let note = f.k.entity(&f.note_id("Tagged").await).await.unwrap();
    let ml = tag_id(&f, "ml").await;
    let brandnew = tag_id(&f, "brandnew").await;
    assert_eq!(
        tag_ids(&note),
        [ml.clone(), brandnew.clone()],
        "a missing Tag is created"
    );
    let mut expected = [format!("Tag:{ml}"), format!("Tag:{brandnew}")];
    expected.sort();
    assert_eq!(f.link_targets(&note.id).await, expected);
}

#[tokio::test]
async fn get_note_frontmatter_shows_tags() {
    let f = fixture().await;
    let tags = ["ml".to_string(), "papers".to_string()];
    f.k.create_note("Tagged", "Body.", Some(&tags))
        .await
        .unwrap();
    f.k.create_note("Plain", "Body #ml.", None).await.unwrap();

    let tagged = f.k.get_note(NoteKey::Name("Tagged")).await.unwrap();
    assert!(
        tagged.contains("\ntags: [\"ml\",\"papers\"]\nrelated: []\n---\n"),
        "{tagged}"
    );
    let plain = f.k.get_note(NoteKey::Name("Plain")).await.unwrap();
    assert!(
        plain.contains("\ntags: []\nrelated: []\n---\n"),
        "a body #tag is not one of the note's tags: {plain}"
    );
}

#[tokio::test]
async fn create_note_tags_argument_wins_over_frontmatter() {
    let f = fixture().await;
    let before = f.entity_count().await;
    let md = "---\nname: \"Wins\"\nversion: 1\ntags: [\"a\"]\n---\nBody.";
    f.k.create_note("Wins", md, Some(&[])).await.unwrap();
    let note = f.k.entity(&f.note_id("Wins").await).await.unwrap();
    assert!(tag_ids(&note).is_empty(), "{:?}", note.properties);
    assert_eq!(
        f.entity_count().await,
        before + 1,
        "no Tag \"a\" is created"
    );

    let other = ["b".to_string()];
    f.k.create_note("Other", md, Some(&other)).await.unwrap();
    let note = f.k.entity(&f.note_id("Other").await).await.unwrap();
    assert_eq!(tag_ids(&note), [tag_id(&f, "b").await]);

    f.k.create_note("Copied", md, None).await.unwrap();
    let note = f.k.entity(&f.note_id("Copied").await).await.unwrap();
    assert_eq!(
        tag_ids(&note),
        [tag_id(&f, "a").await],
        "without the argument, the frontmatter's tags are used"
    );
}

#[tokio::test]
async fn create_note_rejects_malformed_frontmatter_tags() {
    let f = fixture().await;
    let before = f.entity_count().await;
    for value in ["ml", "[\"ml\", 3]", "[ml]"] {
        let md = format!("---\nname: \"Bad\"\ntags: {value}\n---\nAbout #x.");
        let failure = err(f.k.create_note("Bad", &md, None).await);
        assert!(
            failure.contains("tags:") && failure.contains("JSON list"),
            "{failure}"
        );
    }
    assert_eq!(f.entity_count().await, before, "not even the #x Tag exists");
    assert!(err(f.k.get_note(NoteKey::Name("Bad")).await).starts_with("No note named"));
}

#[tokio::test]
async fn tag_entity_adds_and_removes() {
    let f = fixture().await;
    f.create("Tag", "ml").await;
    f.k.create_note("X", "Body.", None).await.unwrap();

    let added = f.k.tag_entity("X", &tag_args(&["ml", "new"]), &[]).await;
    assert_eq!(added.unwrap(), "Tags on \"X\": ml, new.");
    let note = f.k.entity(&f.note_id("X").await).await.unwrap();
    assert_eq!(
        tag_ids(&note),
        [tag_id(&f, "ml").await, tag_id(&f, "new").await],
        "`new` is created"
    );

    let count = f.entity_count().await;
    let removed = f.k.tag_entity("X", &[], &tag_args(&["ml", "absent"])).await;
    assert_eq!(removed.unwrap(), "Tags on \"X\": new.");
    let note = f.k.entity(&f.note_id("X").await).await.unwrap();
    assert_eq!(tag_ids(&note), [tag_id(&f, "new").await]);
    assert_eq!(f.entity_count().await, count, "removing creates no Tag");
    assert!(f
        .k
        .get_note(NoteKey::Name("X"))
        .await
        .unwrap()
        .contains("\ntags: [\"new\"]\n"));

    let cleared = f.k.tag_entity("X", &[], &tag_args(&["NEW"])).await;
    assert_eq!(cleared.unwrap(), "Tags on \"X\": none.");
    let again = f.k.tag_entity("X", &tag_args(&["ML", "ml"]), &[]).await;
    assert_eq!(again.unwrap(), "Tags on \"X\": ml.", "repeats are folded");
}

#[tokio::test]
async fn tag_entity_keeps_a_concurrent_tag() {
    let f = fixture().await;
    f.k.create_note("X", "Body.", None).await.unwrap();
    let id = f.note_id("X").await;

    f.k.tag_entity("X", &tag_args(&["a"]), &[]).await.unwrap();
    f.create("Tag", "b").await;
    let req = AddRelationTargetRequest {
        entity_id: id.clone(),
        property_id: "tags".into(),
        target_id: tag_id(&f, "b").await,
    };
    f.entity
        .add_relation_target(Request::new(req))
        .await
        .unwrap();

    let result = f.k.tag_entity("X", &tag_args(&["c"]), &[]).await;
    assert_eq!(result.unwrap(), "Tags on \"X\": a, b, c.");
    let result = f.k.tag_entity("X", &[], &tag_args(&["a"])).await;
    assert_eq!(result.unwrap(), "Tags on \"X\": b, c.");
}

#[tokio::test]
async fn tag_entity_errors() {
    let f = fixture().await;
    let before = f.entity_count().await;
    let missing = err(f.k.tag_entity("Ghost", &tag_args(&["x"]), &[]).await);
    assert!(missing.contains("No note named"), "{missing}");
    assert_eq!(f.entity_count().await, before, "no note and no Tag");

    f.k.create_note("X", "Body.", None).await.unwrap();
    let before = f.entity_count().await;
    let empty = err(f.k.tag_entity("X", &[], &[]).await);
    assert!(empty.contains("at least one"), "{empty}");
    let both = err(f
        .k
        .tag_entity("X", &tag_args(&["fresh", "ML"]), &tag_args(&["ml"]))
        .await);
    assert!(
        both.contains("\"ML\"") && both.contains("both add and remove"),
        "{both}"
    );
    assert_eq!(f.entity_count().await, before, "nothing was written");
    let note = f.k.entity(&f.note_id("X").await).await.unwrap();
    assert!(tag_ids(&note).is_empty());
}

#[tokio::test]
async fn a_body_hashtag_references_the_tag_without_tagging_the_note() {
    let f = fixture().await;
    f.k.create_note("Mentions", "About #x.", None)
        .await
        .unwrap();
    let note = f.k.entity(&f.note_id("Mentions").await).await.unwrap();
    assert!(tag_ids(&note).is_empty(), "{:?}", note.properties);
    let x = tag_id(&f, "x").await;
    let links: Vec<(&str, &str)> = note
        .links
        .iter()
        .map(|l| {
            (
                l.source_property_id.as_str(),
                l.target.as_ref().unwrap().id.as_str(),
            )
        })
        .collect();
    assert_eq!(links, [("content", x.as_str())]);
}
