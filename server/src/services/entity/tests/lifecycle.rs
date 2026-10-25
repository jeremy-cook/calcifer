use super::*;

#[tokio::test]
async fn create_by_intent_mints_id_defaults_and_name() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let mut rx = svc.hub.subscribe();

    let first = create(&svc, with_facets(untitled("Note"), &["todo"])).await;
    let second = create(&svc, with_facets(untitled("Note"), &["todo"])).await;

    assert!(uuid::Uuid::parse_str(&first.id).is_ok(), "{}", first.id);
    assert_ne!(first.id, second.id);
    assert_eq!(first.structure_type, "Note");
    // Note names aren't unique, so the default may repeat.
    assert_eq!(first.name, "Untitled Note");
    assert_eq!(second.name, "Untitled Note");
    assert_eq!(select_value(&first, "todo.status").as_deref(), Some("open"));
    assert_eq!(
        select_value(&first, "todo.priority").as_deref(),
        Some("none")
    );
    // No stored value for the rich-text `content` (I-22), here or in the DB.
    assert!(!first.properties.contains_key("content"));
    assert!(stored_property_ids(&pool, &first.id)
        .await
        .iter()
        .all(|id| id != "content"));
    // Server-owned fields: no links or dates until content is saved (ADR 3).
    assert!(first.links.is_empty());
    assert!(first.referenced_dates.is_empty());
    assert_eq!(millis(first.created_at), millis(first.updated_at));
    assert_eq!(fts_name(&pool, &first.id).await, "Untitled Note");
    assert_eq!(upserted_ids(&mut rx), [first.id.clone(), second.id.clone()]);

    // A given name is trimmed, and given properties win over the defaults.
    let named = create(
        &svc,
        with_select(todo("  Water plants "), "todo.priority", "high"),
    )
    .await;
    assert_eq!(named.name, "Water plants");
    assert_eq!(
        select_value(&named, "todo.priority").as_deref(),
        Some("high")
    );
    assert_eq!(select_value(&named, "todo.status").as_deref(), Some("open"));

    // A blank name is no name.
    let blank = create(&svc, note("  ")).await;
    assert_eq!(blank.name, "Untitled Note");
}

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

#[tokio::test]
async fn delete_removes_key_rows() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let label = create(&svc, tag("urgent")).await;
    let day = daily_note(&svc, "2026-06-13").await;
    assert_eq!(key_row_count(&pool).await, 2);

    delete(&svc, &label.id).await;
    delete(&svc, &day.id).await;

    assert_eq!(key_row_count(&pool).await, 0);
}

#[tokio::test]
async fn delete_removes_richtext_chunks_and_embeddings() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let gone = create(&svc, note("Gone")).await;
    let kept = create(&svc, note("Kept")).await;
    for entity in [&gone, &kept] {
        put_content(
            &pool,
            &entity.id,
            &[r#"{"type":"text","text":"body"}"#.to_string()],
        )
        .await;
        // Embeddings are disabled in tests, so store a chunk and its
        // vector as the embed worker would.
        let chunk_id: i64 = sqlx::query_scalar(
            "INSERT INTO chunks (entity_id, chunk_index, text) VALUES (?, 0, '') RETURNING id",
        )
        .bind(&entity.id)
        .fetch_one(&pool)
        .await
        .expect("insert chunk");
        sqlx::query("INSERT INTO entity_vec (id, embedding) VALUES (?, ?)")
            .bind(chunk_id)
            .bind(crate::embed::vector_blob(
                &[0.0; crate::embed::provider::EMBED_DIM],
            ))
            .execute(&pool)
            .await
            .expect("insert vector");
    }

    delete(&svc, &gone.id).await;

    for (table, sql) in [
        ("richtext", "SELECT entity_id FROM richtext"),
        ("chunks", "SELECT entity_id FROM chunks"),
        (
            "entity_vec",
            "SELECT c.entity_id FROM entity_vec v LEFT JOIN chunks c ON c.id = v.id",
        ),
    ] {
        let owners: Vec<Option<String>> =
            sqlx::query_scalar(sql).fetch_all(&pool).await.expect(table);
        assert_eq!(owners, [Some(kept.id.clone())], "{table}");
    }
}

#[tokio::test]
async fn delete_removes_the_fts_row_and_its_rowid() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let gone = create(&svc, note("Gone")).await;
    let kept = create(&svc, note("Kept")).await;

    delete(&svc, &gone.id).await;

    let fts: Vec<String> = sqlx::query_scalar("SELECT entity_id FROM entity_fts")
        .fetch_all(&pool)
        .await
        .expect("fts rows");
    assert_eq!(fts, vec![kept.id.clone()]);
    let rowids: Vec<String> = sqlx::query_scalar("SELECT entity_id FROM entity_fts_rowids")
        .fetch_all(&pool)
        .await
        .expect("rowid rows");
    assert_eq!(rowids, vec![kept.id.clone()]);
}

// I-26: a DailyNote is named for its date, so Rename refuses it.
#[tokio::test]
async fn rename_of_a_daily_note_is_failed_precondition() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let day = resolve(&svc, "", by_date("2026-06-13"), true)
        .await
        .expect("create daily note")
        .entity
        .expect("entity");

    let err = rename(&svc, &day.id, "Friday")
        .await
        .expect_err("DailyNote names aren't editable");

    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    let stored = svc.load_entity(&day.id).await.expect("load");
    assert_eq!(stored.name, "June 13, 2026");
    assert_eq!(stored.updated_at, day.updated_at);
    assert_eq!(fts_name(&pool, &day.id).await, "June 13, 2026");
}

// I-15's done-when: the agent sets a to-do's status while the browser renames
// it; both edits survive.
#[tokio::test]
async fn rename_concurrent_with_set_property_keeps_both() {
    let svc = entity_service(memory_pool().await);
    let snapshot = create(&svc, todo("Water plants")).await;

    set_property(&svc, &snapshot.id, "todo.status", select("done"))
        .await
        .expect("agent sets status");
    rename(&svc, &snapshot.id, "Water the plants")
        .await
        .expect("browser renames");

    let stored = svc.load_entity(&snapshot.id).await.expect("load");
    assert_eq!(stored.name, "Water the plants");
    assert_eq!(
        select_value(&stored, "todo.status").as_deref(),
        Some("done")
    );
}
