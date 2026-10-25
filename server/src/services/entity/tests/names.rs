use super::*;

#[tokio::test]
async fn create_second_untitled_tag_gets_a_free_name() {
    let svc = entity_service(memory_pool().await);
    // Taken case-insensitively, so this one is skipped too.
    create(&svc, tag("UNTITLED TAG 3")).await;

    let names = [
        create(&svc, untitled("Tag")).await.name,
        create(&svc, untitled("Tag")).await.name,
        create(&svc, untitled("Tag")).await.name,
    ];

    assert_eq!(names, ["Untitled Tag", "Untitled Tag 2", "Untitled Tag 4"]);
}

#[tokio::test]
async fn rename_changes_only_the_name() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let entity = create(
        &svc,
        with_relations(
            with_select(todo("Before"), "todo.priority", "high"),
            "tags",
            &[(&urgent.id, "Tag")],
        ),
    )
    .await;
    let mut rx = svc.hub.subscribe();

    let renamed = rename(&svc, &entity.id, "After").await.expect("rename");

    assert_eq!(renamed.name, "After");
    // No write can change an entity's type (I-3).
    assert_eq!(renamed.structure_type, "Note");
    assert_eq!(renamed.properties, entity.properties);
    assert_eq!(renamed.links, entity.links);
    assert_eq!(renamed.created_at, entity.created_at);
    assert!(millis(renamed.updated_at) >= millis(entity.updated_at));
    let stored = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(stored, renamed);
    assert_eq!(fts_name(&pool, &entity.id).await, "After");
    assert_eq!(upserted_ids(&mut rx), std::slice::from_ref(&entity.id));
}

#[tokio::test]
async fn rename_trims_the_name() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let entity = create(&svc, note("Before")).await;

    let renamed = rename(&svc, &entity.id, "  After \n")
        .await
        .expect("rename");

    assert_eq!(renamed.name, "After");
    assert_eq!(fts_name(&pool, &entity.id).await, "After");
}

#[tokio::test]
async fn rename_to_a_blank_name_is_invalid() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let entity = create(&svc, note("Kept")).await;

    for blank in ["", "   ", "\t\n"] {
        let err = rename(&svc, &entity.id, blank)
            .await
            .expect_err("blank name");
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "{blank:?}");
    }
    let stored = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(stored, entity);
    assert_eq!(fts_name(&pool, &entity.id).await, "Kept");
}

// I-61: the agent's link syntax can't express "|" or "]]" in a name.
#[tokio::test]
async fn names_with_a_pipe_or_double_bracket_are_invalid() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let kept = create(&svc, note("Kept")).await;
    let before = entity_count(&pool).await;

    for name in ["A|B", "x]]y", "  A|B  ", "a]]", "|"] {
        let err = try_create(&svc, note(name))
            .await
            .expect_err("create refuses it");
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "{name:?}");
        assert_eq!(
            err.message(),
            r#"names can't contain "|" or "]]""#,
            "{name:?}"
        );

        let err = rename(&svc, &kept.id, name)
            .await
            .expect_err("rename refuses it");
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "{name:?}");

        let err = resolve(&svc, "Note", by_name(name), true)
            .await
            .expect_err("a creating resolve refuses it");
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "{name:?}");
    }
    assert_eq!(entity_count(&pool).await, before, "nothing was created");
    let stored = svc.load_entity(&kept.id).await.expect("load");
    assert_eq!(stored, kept, "the rename wrote nothing");
    assert_eq!(fts_name(&pool, &kept.id).await, "Kept");

    // A tag's name is checked on every path too.
    let err = try_create(&svc, tag("a|b")).await.expect_err("tag name");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    let err = resolve(&svc, "Tag", by_name("a]]b"), true)
        .await
        .expect_err("tag name");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // A lone bracket doesn't close a link.
    let made = create(&svc, note("a[b]c")).await;
    assert_eq!(made.name, "a[b]c");
    let renamed = rename(&svc, &kept.id, "[x]").await.expect("rename");
    assert_eq!(renamed.name, "[x]");
    let resolved = resolve(&svc, "Note", by_name("a]b[c"), true)
        .await
        .expect("resolve");
    assert!(resolved.created);
}

#[tokio::test]
async fn resolving_such_a_name_without_create_is_not_found() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let before = entity_count(&pool).await;

    for name in ["A|B", "x]]y"] {
        let err = resolve(&svc, "Note", by_name(name), false)
            .await
            .expect_err("no such note");
        assert_eq!(err.code(), tonic::Code::NotFound, "{name:?}");
    }
    assert_eq!(entity_count(&pool).await, before);
}

// I-8: an unknown id is NotFound, and nothing is written for it.
#[tokio::test]
async fn rename_missing_id_is_not_found() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = rename(&svc, "no-such-entity", "Ghost")
        .await
        .expect_err("rename of a missing id should fail");

    assert_eq!(err.code(), tonic::Code::NotFound);
    let fts_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity_fts WHERE entity_id = ?")
        .bind("no-such-entity")
        .fetch_one(&pool)
        .await
        .expect("count fts");
    assert_eq!(fts_rows, 0);
}

#[tokio::test]
async fn rename_onto_a_taken_tag_name_is_already_exists() {
    let svc = entity_service(memory_pool().await);
    create(&svc, tag("urgent")).await;
    let other = create(&svc, tag("later")).await;

    let err = rename(&svc, &other.id, "URGENT")
        .await
        .expect_err("tag names are unique");

    assert_eq!(err.code(), tonic::Code::AlreadyExists);
    assert_eq!(err.message(), r#"a Tag named "URGENT" already exists"#);
    let stored = svc.load_entity(&other.id).await.expect("load");
    assert_eq!(stored.name, "later");
}

// I-25: the message names the Tag clash, not a DailyNote date.
#[tokio::test]
async fn create_onto_a_taken_tag_name_is_already_exists() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    create(&svc, tag("urgent")).await;

    let err = try_create(&svc, tag("Urgent"))
        .await
        .expect_err("tag names are unique");

    assert_eq!(err.code(), tonic::Code::AlreadyExists);
    assert_eq!(err.message(), r#"a Tag named "Urgent" already exists"#);
    assert_eq!(entity_count(&pool).await, 1);
}

// ADR 10: a Tag name key folds Unicode case, not just ASCII as NOCASE did.
#[tokio::test]
async fn tag_names_differing_in_non_ascii_case_conflict() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    create(&svc, tag("Café")).await;

    let err = try_create(&svc, tag("CAFÉ"))
        .await
        .expect_err("tag names are unique");

    assert_eq!(err.code(), tonic::Code::AlreadyExists);
    assert_eq!(err.message(), r#"a Tag named "CAFÉ" already exists"#);
    assert_eq!(entity_count(&pool).await, 1);
}

#[tokio::test]
async fn rename_moves_a_tag_key_row() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let entity = create(&svc, tag("Reading")).await;
    assert_eq!(
        stored_key(&pool, &entity.id, "name").await.as_deref(),
        Some("reading")
    );

    rename(&svc, &entity.id, " Writing ").await.expect("rename");

    assert_eq!(
        stored_key(&pool, &entity.id, "name").await.as_deref(),
        Some("writing")
    );
    // The old name is free again.
    create(&svc, tag("reading")).await;
    assert_eq!(key_row_count(&pool).await, 2);
}
