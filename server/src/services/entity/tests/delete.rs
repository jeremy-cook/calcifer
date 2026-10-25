use super::*;

#[tokio::test]
async fn delete_strips_the_id_from_relations_values() {
    let svc = entity_service(memory_pool().await);
    let gone = create(&svc, tag("gone")).await;
    let kept = create(&svc, tag("kept")).await;
    let todo = tagged_todo(&svc, &[&gone, &kept]).await;

    delete(&svc, &gone.id).await;

    let stored = svc.load_entity(&todo.id).await.expect("load");
    assert_eq!(relation_ids(&stored, "tags"), Some(vec![kept.id.clone()]));
    assert_eq!(
        link_targets(&stored, "tags"),
        [(kept.id.clone(), "Tag".to_string())]
    );
    assert!(millis(stored.updated_at) > millis(todo.updated_at));
}

#[tokio::test]
async fn delete_clears_a_relations_value_left_empty() {
    let svc = entity_service(memory_pool().await);
    let only = create(&svc, tag("only")).await;
    let todo = tagged_todo(&svc, &[&only]).await;

    delete(&svc, &only.id).await;

    let stored = svc.load_entity(&todo.id).await.expect("load");
    assert!(!stored.properties.contains_key("tags"));
    assert!(link_targets(&stored, "tags").is_empty());
    assert!(millis(stored.updated_at) > millis(todo.updated_at));
}

// No structure declares a single `relation` property, so the value is one
// stored before I-79.
#[tokio::test]
async fn delete_clears_a_single_relation_value() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let target = create(&svc, note("Target")).await;
    let source = create(&svc, note("Source")).await;
    store_legacy_value(
        &pool,
        &source.id,
        "parent",
        property_value::Value::Relation(EntityRef {
            id: target.id.clone(),
            structure_type: String::new(),
        }),
    )
    .await;
    store_legacy_link(&pool, &source.id, "parent", &target).await;
    let source = svc.load_entity(&source.id).await.expect("load");
    assert_eq!(link_targets(&source, "parent").len(), 1);
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    delete(&svc, &target.id).await;

    let stored = svc.load_entity(&source.id).await.expect("load");
    assert!(!stored.properties.contains_key("parent"));
    assert!(stored.links.is_empty());
    assert!(millis(stored.updated_at) > millis(source.updated_at));
}

#[tokio::test]
async fn delete_publishes_upserts_for_stripped_entities() {
    let svc = entity_service(memory_pool().await);
    let gone = create(&svc, tag("gone")).await;
    let kept = create(&svc, tag("kept")).await;
    let todo = tagged_todo(&svc, &[&gone, &kept]).await;
    let mut rx = svc.hub.subscribe();

    delete(&svc, &gone.id).await;

    let first = rx.recv().await.expect("deleted event");
    assert_eq!(
        first.event,
        Some(watch_entities_response::Event::DeletedId(gone.id.clone()))
    );
    let second = rx.recv().await.expect("upserted event");
    let Some(watch_entities_response::Event::Upserted(upserted)) = second.event else {
        panic!("expected an upsert, got {second:?}");
    };
    assert_eq!(upserted.id, todo.id);
    assert_eq!(relation_ids(&upserted, "tags"), Some(vec![kept.id.clone()]));
    assert!(rx.try_recv().is_err(), "no further events");
}

#[tokio::test]
async fn delete_leaves_rich_text_mentions_out_of_properties() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let gone = create(&svc, tag("gone")).await;
    let mentioner = create(&svc, todo("Mentions the tag")).await;
    let doc = format!(
        r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"mention","attrs":{{"id":"{}","structureType":"Tag"}}}}]}}]}}"#,
        gone.id
    );
    richtext_service(pool.clone(), WatchHub::new())
        .put_rich_text(Request::new(PutRichTextRequest {
            entity_id: mentioner.id.clone(),
            property_id: "content".to_string(),
            doc,
            expected_updated_at: None,
        }))
        .await
        .expect("put content");
    let before = svc.load_entity(&mentioner.id).await.expect("load");
    assert_eq!(link_targets(&before, "content").len(), 1);
    let mut rx = svc.hub.subscribe();

    delete(&svc, &gone.id).await;

    let after = svc.load_entity(&mentioner.id).await.expect("load");
    assert_eq!(after.properties, before.properties);
    assert_eq!(after.updated_at, before.updated_at);
    let event = rx.recv().await.expect("deleted event");
    assert_eq!(
        event.event,
        Some(watch_entities_response::Event::DeletedId(gone.id.clone()))
    );
    assert!(rx.try_recv().is_err(), "no upsert for a rich-text mention");
}

#[tokio::test]
async fn delete_removes_facet_rows() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
    let _kept = create(&svc, with_facets(note("Kept"), &["todo"])).await;
    assert_eq!(facet_row_count(&pool).await, 2);

    delete(&svc, &ship.id).await;

    assert_eq!(facet_row_count(&pool).await, 1);
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
